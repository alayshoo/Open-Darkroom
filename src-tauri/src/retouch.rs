// src-tauri/src/retouch.rs
//
// Heal and clone strokes on export, through the same retouch.wgsl the preview
// bakes them with. The geometry mirrors src/lib/types/retouch.ts so both cut a
// stroke into the same tiles; the strokes land in a sparse cache of linear
// tiles that every region the export renders is read through.

use std::collections::HashMap;

use serde::Deserialize;
use wgpu::util::DeviceExt;

use crate::color::{linearize_u16_f32, LINEAR_F32_BYTES_PER_PIXEL};
use crate::shaders::RETOUCH_WGSL;

/// Output tiles a stroke is applied in. Must match RETOUCH_TILE in retouch.ts.
pub const RETOUCH_TILE: i64 = 512;

/// Most taps on either side of a blur. Must match MAX_BLUR_TAPS in retouch.ts.
pub const MAX_BLUR_TAPS: i64 = 64;

/// Size of the `RetouchPass` uniform in retouch.wgsl.
pub const RETOUCH_UNIFORM_BYTES: usize = 80;

/// Side of the blocks the retouched pixels are kept in.
const CACHE_TILE: u32 = 128;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetouchMode {
    Heal,
    Clone,
}

/// One stroke as the frontend stores it: fractions of the source's width and
/// height, and a radius as a fraction of its long edge.
#[derive(Debug, Clone, Deserialize)]
pub struct RetouchPayload {
    pub mode: RetouchMode,
    pub points: Vec<[f64; 2]>,
    pub radius: f64,
    pub feather: f64,
    pub opacity: f64,
    pub offset: [f64; 2],
}

impl RetouchPayload {
    fn is_valid(&self) -> bool {
        !self.points.is_empty()
            && self.points.iter().flatten().all(|v| v.is_finite())
            && self.offset.iter().all(|v| v.is_finite())
            && self.radius.is_finite()
            && self.radius > 0.0
    }
}

/// `[x0, y0, x1, y1]` in texels, half open.
pub type TexelBox = [i64; 4];

/// A stroke laid on an image `width` × `height` texels.
#[derive(Debug, Clone)]
pub struct PlacedRetouch {
    pub mode: RetouchMode,
    pub points: Vec<[f64; 2]>,
    pub radius: f64,
    pub feather: f64,
    pub opacity: f64,
    pub offset: [i64; 2],
}

// JavaScript's Math.round, which rounds halves up rather than away from zero.
fn js_round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

pub fn place_retouch(r: &RetouchPayload, width: u32, height: u32) -> PlacedRetouch {
    let (w, h) = (width as f64, height as f64);
    PlacedRetouch {
        mode: r.mode,
        points: r.points.iter().map(|[x, y]| [x * w, y * h]).collect(),
        radius: r.radius * w.max(h),
        feather: r.feather.clamp(0.0, 1.0),
        opacity: r.opacity.clamp(0.0, 1.0),
        offset: [js_round(r.offset[0] * w), js_round(r.offset[1] * h)],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlurSpan {
    pub sigma: f64,
    pub reach: i64,
    pub step: i64,
    pub taps: i64,
}

pub fn blur_span(radius: f64) -> BlurSpan {
    let sigma = (radius * 0.5).max(1.0);
    let needed = (3.0 * sigma).ceil() as i64;
    let step = (needed + MAX_BLUR_TAPS - 1) / MAX_BLUR_TAPS;
    let taps = (needed + step - 1) / step;
    // Every tap lands inside the margin, so a texel blurs the same whichever tile it is in.
    BlurSpan { sigma, reach: taps * step, step, taps }
}

fn clamp_box([x0, y0, x1, y1]: TexelBox, width: u32, height: u32) -> Option<TexelBox> {
    let b = [x0.max(0), y0.max(0), x1.min(width as i64), y1.min(height as i64)];
    (b[2] > b[0] && b[3] > b[1]).then_some(b)
}

pub fn retouch_bounds(r: &PlacedRetouch, width: u32, height: u32) -> Option<TexelBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for [x, y] in &r.points {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let reach = r.radius + 1.0;
    clamp_box(
        [
            (x0 - reach).floor() as i64,
            (y0 - reach).floor() as i64,
            (x1 + reach).ceil() as i64,
            (y1 + reach).ceil() as i64,
        ],
        width,
        height,
    )
}

pub fn stroke_segments(points: &[[f64; 2]]) -> Vec<[f64; 4]> {
    if points.len() == 1 {
        let [x, y] = points[0];
        return vec![[x, y, x, y]];
    }
    points.windows(2).map(|p| [p[0][0], p[0][1], p[1][0], p[1][1]]).collect()
}

// Whether the segment crosses the box grown by `by`, clipped Liang–Barsky style.
fn segment_near_box(s: &[f64; 4], b: &TexelBox, by: f64) -> bool {
    let [ax, ay, bx, by_] = *s;
    let (dx, dy) = (bx - ax, by_ - ay);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let edges = [
        (-dx, ax - (b[0] as f64 - by)),
        (dx, b[2] as f64 + by - ax),
        (-dy, ay - (b[1] as f64 - by)),
        (dy, b[3] as f64 + by - ay),
    ];
    for (p, q) in edges {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Clone)]
pub struct RetouchTile {
    pub target: TexelBox,
    pub work: TexelBox,
    pub segments: Vec<[f64; 4]>,
}

pub fn retouch_tiles(r: &PlacedRetouch, width: u32, height: u32) -> Vec<RetouchTile> {
    let Some(bounds) = retouch_bounds(r, width, height) else {
        return Vec::new();
    };
    let margin = if r.mode == RetouchMode::Heal { blur_span(r.radius).reach } else { 0 };
    let segments = stroke_segments(&r.points);
    let near = r.radius + 1.0;
    let mut tiles = Vec::new();
    let mut y = bounds[1];
    while y < bounds[3] {
        let mut x = bounds[0];
        while x < bounds[2] {
            let target = [x, y, (x + RETOUCH_TILE).min(bounds[2]), (y + RETOUCH_TILE).min(bounds[3])];
            if segments.iter().any(|s| segment_near_box(s, &target, near)) {
                let work = clamp_box(
                    [target[0] - margin, target[1] - margin, target[2] + margin, target[3] + margin],
                    width,
                    height,
                )
                .expect("a tile's own box is on the image");
                let reaching = if margin > 0 { work } else { target };
                let segments = segments.iter().copied().filter(|s| segment_near_box(s, &reaching, near)).collect();
                tiles.push(RetouchTile { target, work, segments });
            }
            x += RETOUCH_TILE;
        }
        y += RETOUCH_TILE;
    }
    tiles
}

/// What a tile reads from the image: its work box, and that box moved by the offset.
pub fn tile_reads(tile: &RetouchTile, offset: [i64; 2], width: u32, height: u32) -> Option<TexelBox> {
    let [x0, y0, x1, y1] = tile.work;
    let [dx, dy] = offset;
    clamp_box([x0.min(x0 + dx), y0.min(y0 + dy), x1.max(x1 + dx), y1.max(y1 + dy)], width, height)
}

pub fn retouch_uniform(
    r: &PlacedRetouch,
    tile: &RetouchTile,
    image_origin: [i64; 2],
    width: u32,
    height: u32,
) -> [u8; RETOUCH_UNIFORM_BYTES] {
    let span = blur_span(r.radius);
    let floats = [
        tile.target[0] as f32,
        tile.target[1] as f32,
        tile.work[0] as f32,
        tile.work[1] as f32,
        (tile.work[2] - tile.work[0]) as f32,
        (tile.work[3] - tile.work[1]) as f32,
        image_origin[0] as f32,
        image_origin[1] as f32,
        width as f32,
        height as f32,
        r.offset[0] as f32,
        r.offset[1] as f32,
        r.radius as f32,
        r.feather as f32,
        r.opacity as f32,
        span.sigma as f32,
        span.step as f32,
    ];
    let uints = [span.taps as u32, tile.segments.len() as u32, (r.mode == RetouchMode::Heal) as u32];
    let mut bytes = [0u8; RETOUCH_UNIFORM_BYTES];
    for (i, v) in floats.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    for (i, v) in uints.iter().enumerate() {
        let at = (floats.len() + i) * 4;
        bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

// ── The linear source, with the strokes laid over it ──────────────────────────

/// The image as the export reads it: linear RGBA f32, with every retouched
/// block taking the place of the untouched pixels under it.
pub struct LinearSource<'a> {
    pixels: &'a [u16],
    width: u32,
    height: u32,
    channels: usize,
    luts: &'a [Vec<f32>; 3],
    blocks: HashMap<(u32, u32), Vec<u8>>,
}

impl<'a> LinearSource<'a> {
    pub fn new(pixels: &'a [u16], width: u32, height: u32, channels: usize, luts: &'a [Vec<f32>; 3]) -> Self {
        Self { pixels, width, height, channels, luts, blocks: HashMap::new() }
    }

    fn untouched(&self, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
        let (width, channels) = (self.width as usize, self.channels);
        if x0 == 0 && w == self.width {
            let start = y0 as usize * width * channels;
            return linearize_u16_f32(&self.pixels[start..start + h as usize * width * channels], channels, self.luts);
        }
        let row = w as usize * channels;
        let mut region = Vec::with_capacity(row * h as usize);
        for y in y0..y0 + h {
            let at = (y as usize * width + x0 as usize) * channels;
            region.extend_from_slice(&self.pixels[at..at + row]);
        }
        linearize_u16_f32(&region, channels, self.luts)
    }

    fn block_box(&self, bx: u32, by: u32) -> (u32, u32, u32, u32) {
        let (x, y) = (bx * CACHE_TILE, by * CACHE_TILE);
        (x, y, CACHE_TILE.min(self.width - x), CACHE_TILE.min(self.height - y))
    }

    /// `w` × `h` texels from (`x0`, `y0`), as LE f32 RGBA.
    pub fn read(&self, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
        let mut out = self.untouched(x0, y0, w, h);
        if self.blocks.is_empty() {
            return out;
        }
        let px = LINEAR_F32_BYTES_PER_PIXEL;
        for by in y0 / CACHE_TILE..=(y0 + h - 1) / CACHE_TILE {
            for bx in x0 / CACHE_TILE..=(x0 + w - 1) / CACHE_TILE {
                let Some(block) = self.blocks.get(&(bx, by)) else { continue };
                let (bx0, by0, bw, bh) = self.block_box(bx, by);
                let (cx0, cx1) = (bx0.max(x0), (bx0 + bw).min(x0 + w));
                let (cy0, cy1) = (by0.max(y0), (by0 + bh).min(y0 + h));
                let span = (cx1 - cx0) as usize * px;
                for y in cy0..cy1 {
                    let src = ((y - by0) as usize * bw as usize + (cx0 - bx0) as usize) * px;
                    let dst = ((y - y0) as usize * w as usize + (cx0 - x0) as usize) * px;
                    out[dst..dst + span].copy_from_slice(&block[src..src + span]);
                }
            }
        }
        out
    }

    /// Every block a stroke has written, as its box `[x0, y0, w, h]` and its
    /// pixels in LE f32 RGBA.
    pub fn blocks(&self) -> impl Iterator<Item = ([u32; 4], &[u8])> + '_ {
        self.blocks.iter().map(|(&(bx, by), data)| {
            let (x, y, w, h) = self.block_box(bx, by);
            ([x, y, w, h], data.as_slice())
        })
    }

    fn write(&mut self, x0: u32, y0: u32, w: u32, h: u32, data: &[u8]) {
        let px = LINEAR_F32_BYTES_PER_PIXEL;
        for by in y0 / CACHE_TILE..=(y0 + h - 1) / CACHE_TILE {
            for bx in x0 / CACHE_TILE..=(x0 + w - 1) / CACHE_TILE {
                let (bx0, by0, bw, bh) = self.block_box(bx, by);
                if !self.blocks.contains_key(&(bx, by)) {
                    let fresh = self.untouched(bx0, by0, bw, bh);
                    self.blocks.insert((bx, by), fresh);
                }
                let block = self.blocks.get_mut(&(bx, by)).unwrap();
                let (cx0, cx1) = (bx0.max(x0), (bx0 + bw).min(x0 + w));
                let (cy0, cy1) = (by0.max(y0), (by0 + bh).min(y0 + h));
                let span = (cx1 - cx0) as usize * px;
                for y in cy0..cy1 {
                    let dst = ((y - by0) as usize * bw as usize + (cx0 - bx0) as usize) * px;
                    let src = ((y - y0) as usize * w as usize + (cx0 - x0) as usize) * px;
                    block[dst..dst + span].copy_from_slice(&data[src..src + span]);
                }
            }
        }
    }
}

// ── Applying the strokes ──────────────────────────────────────────────────────

struct Passes {
    layout: wgpu::BindGroupLayout,
    prep: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    apply: wgpu::RenderPipeline,
}

fn create_passes(device: &wgpu::Device) -> Passes {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let buffer = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("retouch_bgl"),
        entries: &[
            texture(0),
            texture(1),
            buffer(2, wgpu::BufferBindingType::Uniform),
            buffer(3, wgpu::BufferBindingType::Storage { read_only: true }),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("retouch"),
        source: wgpu::ShaderSource::Wgsl(RETOUCH_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("retouch_layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = |entry_point: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry_point),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    };
    Passes {
        prep: pipeline("fs_prep"),
        blur_h: pipeline("fs_blur_h"),
        blur_v: pipeline("fs_blur_v"),
        apply: pipeline("fs_apply"),
        layout,
    }
}

fn texture(device: &wgpu::Device, label: &str, width: u32, height: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage,
        view_formats: &[],
    })
}

fn run_pass(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    target: &wgpu::Texture,
    bind_group: &wgpu::BindGroup,
) {
    let view = target.create_view(&Default::default());
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("retouch_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..6, 0..1);
}

/// One tile of a stroke, rendered against `source` as it stands, read back as
/// LE f32 RGBA over the tile's target box.
fn render_tile(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &Passes,
    source: &LinearSource,
    stroke: &PlacedRetouch,
    tile: &RetouchTile,
) -> Result<Vec<u8>, String> {
    let (width, height) = (source.width, source.height);
    let reads = tile_reads(tile, stroke.offset, width, height).expect("a tile's reads include its own box");
    let (rw, rh) = ((reads[2] - reads[0]) as u32, (reads[3] - reads[1]) as u32);
    let image = texture(device, "retouch_image", rw, rh, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &image,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &source.read(reads[0] as u32, reads[1] as u32, rw, rh),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(rw * LINEAR_F32_BYTES_PER_PIXEL as u32),
            rows_per_image: None,
        },
        wgpu::Extent3d { width: rw, height: rh, depth_or_array_layers: 1 },
    );

    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retouch_uniform"),
        contents: &retouch_uniform(stroke, tile, [reads[0], reads[1]], width, height),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let segment_bytes: Vec<u8> = tile.segments.iter().flatten().flat_map(|v| (*v as f32).to_le_bytes()).collect();
    let segments = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retouch_segments"),
        contents: &segment_bytes,
        usage: wgpu::BufferUsages::STORAGE,
    });

    let image_view = image.create_view(&Default::default());
    let bind = |input: &wgpu::Texture| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retouch_bg"),
            layout: &passes.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&image_view) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&input.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: segments.as_entire_binding() },
            ],
        })
    };

    let mut encoder = device.create_command_encoder(&Default::default());
    let target_usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let matched = if stroke.mode == RetouchMode::Heal {
        let (ww, wh) = ((tile.work[2] - tile.work[0]) as u32, (tile.work[3] - tile.work[1]) as u32);
        let work: Vec<_> = (0..3).map(|_| texture(device, "retouch_work", ww, wh, target_usage)).collect();
        run_pass(&mut encoder, &passes.prep, &work[0], &bind(&image));
        run_pass(&mut encoder, &passes.blur_h, &work[1], &bind(&work[0]));
        run_pass(&mut encoder, &passes.blur_v, &work[2], &bind(&work[1]));
        work.into_iter().nth(2).unwrap()
    } else {
        texture(device, "retouch_blank", 1, 1, wgpu::TextureUsages::TEXTURE_BINDING)
    };

    let (tw, th) = ((tile.target[2] - tile.target[0]) as u32, (tile.target[3] - tile.target[1]) as u32);
    let output = texture(device, "retouch_output", tw, th, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    run_pass(&mut encoder, &passes.apply, &output, &bind(&matched));

    let row = tw * LINEAR_F32_BYTES_PER_PIXEL as u32;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retouch_staging"),
        size: (padded * th) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
        },
        wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let (tx, rx) = std::sync::mpsc::channel();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::MaintainBase::Wait);
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("GPU buffer map failed: {e}"))?;

    let mapped = staging.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((row * th) as usize);
    for y in 0..th as usize {
        let at = y * padded as usize;
        out.extend_from_slice(&mapped[at..at + row as usize]);
    }
    drop(mapped);
    staging.unmap();
    Ok(out)
}

/// Lays `strokes` over `source` in order, each against the image as the
/// strokes before it left it. Strokes that cannot be read are skipped.
pub fn apply_retouch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &mut LinearSource,
    strokes: &[RetouchPayload],
) -> Result<(), String> {
    if strokes.is_empty() {
        return Ok(());
    }
    let passes = create_passes(device);
    for stroke in strokes.iter().filter(|s| s.is_valid()) {
        let placed = place_retouch(stroke, source.width, source.height);
        let tiles = retouch_tiles(&placed, source.width, source.height);
        let mut rendered = Vec::with_capacity(tiles.len());
        for tile in &tiles {
            rendered.push(render_tile(device, queue, &passes, source, &placed, tile)?);
        }
        for (tile, data) in tiles.iter().zip(rendered) {
            let [x0, y0, x1, y1] = tile.target;
            source.write(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32, &data);
        }
    }
    Ok(())
}
