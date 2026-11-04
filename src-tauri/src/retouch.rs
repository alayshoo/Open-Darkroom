// src-tauri/src/retouch.rs
//
// Heal and clone strokes on export, through the same retouch.wgsl the preview
// bakes them with. The geometry mirrors src/lib/types/retouch.ts so both cut a
// stroke into the same tiles; the strokes land in a sparse cache of linear
// tiles that every region the export renders is read through.

use std::collections::{HashMap, VecDeque};

use serde::Deserialize;
use wgpu::util::DeviceExt;

use crate::color::{linearize_u16_f32, LINEAR_F32_BYTES_PER_PIXEL};
use crate::shaders::RETOUCH_WGSL;

/// Output tiles a stroke is applied in. Must match RETOUCH_TILE in retouch.ts.
pub const RETOUCH_TILE: i64 = 512;

/// Most taps on either side of a blur. Must match MAX_BLUR_TAPS in retouch.ts.
pub const MAX_BLUR_TAPS: i64 = 64;

/// Size of the `RetouchPass` uniform in retouch.wgsl.
pub const RETOUCH_UNIFORM_BYTES: usize = 96;

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
#[derive(Debug, Clone, PartialEq, Deserialize)]
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

/// What a tile reads from the image: its work box, and that box moved by the
/// offset when it is on the image.
pub fn tile_reads(tile: &RetouchTile, offset: [i64; 2], width: u32, height: u32) -> (TexelBox, Option<TexelBox>) {
    let [x0, y0, x1, y1] = tile.work;
    let [dx, dy] = offset;
    (tile.work, clamp_box([x0 + dx, y0 + dy, x1 + dx, y1 + dy], width, height))
}

pub fn retouch_uniform(
    r: &PlacedRetouch,
    tile: &RetouchTile,
    image_origin: [i64; 2],
    source_origin: [i64; 2],
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
    for (i, v) in source_origin.iter().enumerate() {
        let at = (floats.len() + uints.len() + i) * 4;
        bytes[at..at + 4].copy_from_slice(&(*v as f32).to_le_bytes());
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
    blocks: RetouchedBlocks,
    journal: Option<BlockJournal>,
}

/// The blocks strokes have written, as LE f32 RGBA, by block.
pub type RetouchedBlocks = HashMap<(u32, u32), Vec<u8>>;

/// What writes overwrote: each block they changed and what it held before,
/// or None where it was untouched.
pub type BlockJournal = HashMap<(u32, u32), Option<Vec<u8>>>;

/// The box `[x0, y0, w, h]` of block `key` on an image `width` × `height`.
pub fn block_box((bx, by): (u32, u32), width: u32, height: u32) -> [u32; 4] {
    let (x, y) = (bx * CACHE_TILE, by * CACHE_TILE);
    [x, y, CACHE_TILE.min(width - x), CACHE_TILE.min(height - y)]
}

impl<'a> LinearSource<'a> {
    pub fn new(pixels: &'a [u16], width: u32, height: u32, channels: usize, luts: &'a [Vec<f32>; 3]) -> Self {
        Self::with_blocks(pixels, width, height, channels, luts, HashMap::new())
    }

    /// The image with `blocks`, which earlier strokes wrote over the same pixels, laid over it.
    pub fn with_blocks(
        pixels: &'a [u16],
        width: u32,
        height: u32,
        channels: usize,
        luts: &'a [Vec<f32>; 3],
        blocks: RetouchedBlocks,
    ) -> Self {
        Self { pixels, width, height, channels, luts, blocks, journal: None }
    }

    pub fn into_blocks(self) -> RetouchedBlocks {
        self.blocks
    }

    /// What block `key` holds, or None while no stroke has written it.
    pub fn block(&self, key: (u32, u32)) -> Option<&[u8]> {
        self.blocks.get(&key).map(Vec::as_slice)
    }

    /// Starts recording what later writes overwrite.
    fn start_journal(&mut self) {
        self.journal = Some(BlockJournal::new());
    }

    /// What was overwritten since [`Self::start_journal`].
    fn take_journal(&mut self) -> BlockJournal {
        self.journal.take().unwrap_or_default()
    }

    /// Puts back what `journal` recorded.
    fn roll_back(&mut self, journal: BlockJournal) {
        for (key, held) in journal {
            match held {
                Some(data) => self.blocks.insert(key, data),
                None => self.blocks.remove(&key),
            };
        }
    }

    fn clear_blocks(&mut self) {
        self.blocks.clear();
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
        let [x, y, w, h] = block_box((bx, by), self.width, self.height);
        (x, y, w, h)
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

    /// Every block a stroke has written, by key, with its pixels in LE f32 RGBA.
    pub fn blocks(&self) -> impl Iterator<Item = ((u32, u32), &[u8])> + '_ {
        self.blocks.iter().map(|(&key, data)| (key, data.as_slice()))
    }

    fn write(&mut self, x0: u32, y0: u32, w: u32, h: u32, data: &[u8]) {
        let px = LINEAR_F32_BYTES_PER_PIXEL;
        for by in y0 / CACHE_TILE..=(y0 + h - 1) / CACHE_TILE {
            for bx in x0 / CACHE_TILE..=(x0 + w - 1) / CACHE_TILE {
                let (bx0, by0, bw, bh) = self.block_box(bx, by);
                if let Some(journal) = &mut self.journal {
                    journal.entry((bx, by)).or_insert_with(|| self.blocks.get(&(bx, by)).cloned());
                }
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

/// The retouch pipelines, built once per device.
pub struct RetouchPasses {
    layout: wgpu::BindGroupLayout,
    prep: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    apply: wgpu::RenderPipeline,
}

impl RetouchPasses {
    pub fn new(device: &wgpu::Device) -> Self {
        create_passes(device)
    }
}

fn create_passes(device: &wgpu::Device) -> RetouchPasses {
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
    RetouchPasses {
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

/// GPU memory the tiles of one submission may hold between them.
const BATCH_BYTES: u64 = 256 << 20;

fn box_size(b: &TexelBox) -> (u32, u32) {
    ((b[2] - b[0]) as u32, (b[3] - b[1]) as u32)
}

fn padded_row(width: u32) -> u32 {
    let row = width * LINEAR_F32_BYTES_PER_PIXEL as u32;
    row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

/// Bytes of GPU memory one tile takes while it renders.
fn tile_bytes(stroke: &PlacedRetouch, tile: &RetouchTile, width: u32, height: u32) -> u64 {
    let (work, source_box) = tile_reads(tile, stroke.offset, width, height);
    let (ww, wh) = box_size(&work);
    let (sw, sh) = source_box.as_ref().map_or((0, 0), box_size);
    let (tw, th) = box_size(&tile.target);
    let px = LINEAR_F32_BYTES_PER_PIXEL as u64;
    let image = ww.max(sw) as u64 * (wh + sh) as u64 * px;
    let blur = if stroke.mode == RetouchMode::Heal { 3 * ww as u64 * wh as u64 * px } else { 0 };
    image + blur + 2 * tw as u64 * th as u64 * px + padded_row(tw) as u64 * th as u64
}

/// Records one tile of a stroke into `encoder`, rendered against `source` as
/// it stands and copied into `staging` from `at`. Returns the textures the
/// commands read, which must outlive the submission.
#[allow(clippy::too_many_arguments)]
fn encode_tile(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &RetouchPasses,
    source: &LinearSource,
    stroke: &PlacedRetouch,
    tile: &RetouchTile,
    encoder: &mut wgpu::CommandEncoder,
    blank: &wgpu::Texture,
    staging: &wgpu::Buffer,
    at: u64,
) -> Vec<wgpu::Texture> {
    let (width, height) = (source.width, source.height);
    // The work box on top, and the source box under it, so the image holds only what the tile reads.
    let (work, source_box) = tile_reads(tile, stroke.offset, width, height);
    let (ww, wh) = box_size(&work);
    let (sw, sh) = source_box.as_ref().map_or((0, 0), box_size);
    let image = texture(device, "retouch_image", ww.max(sw), wh + sh, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    let upload = |b: &TexelBox, y: u32| {
        let (bw, bh) = box_size(b);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &source.read(b[0] as u32, b[1] as u32, bw, bh),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bw * LINEAR_F32_BYTES_PER_PIXEL as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: bw, height: bh, depth_or_array_layers: 1 },
        );
    };
    upload(&work, 0);
    if let Some(b) = &source_box {
        upload(b, wh);
    }
    let source_origin = source_box.map_or([0, 0], |b| [b[0], b[1] - wh as i64]);

    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("retouch_uniform"),
        contents: &retouch_uniform(stroke, tile, [work[0], work[1]], source_origin, width, height),
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

    let mut kept = Vec::new();
    let target_usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let matched = if stroke.mode == RetouchMode::Heal {
        let blur: Vec<_> = (0..3).map(|_| texture(device, "retouch_work", ww, wh, target_usage)).collect();
        run_pass(encoder, &passes.prep, &blur[0], &bind(blank));
        run_pass(encoder, &passes.blur_h, &blur[1], &bind(&blur[0]));
        run_pass(encoder, &passes.blur_v, &blur[2], &bind(&blur[1]));
        let matched = blur[2].clone();
        kept.extend(blur);
        matched
    } else {
        blank.clone()
    };

    let (tw, th) = box_size(&tile.target);
    let output = texture(device, "retouch_output", tw, th, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    run_pass(encoder, &passes.apply, &output, &bind(&matched));
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: staging,
            layout: wgpu::TexelCopyBufferLayout { offset: at, bytes_per_row: Some(padded_row(tw)), rows_per_image: None },
        },
        wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
    );
    kept.extend([image, output]);
    kept
}

/// `tiles`, rendered in one submission and read back as LE f32 RGBA over
/// each tile's target box.
fn render_batch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &RetouchPasses,
    source: &LinearSource,
    stroke: &PlacedRetouch,
    tiles: &[RetouchTile],
) -> Result<Vec<Vec<u8>>, String> {
    let offsets: Vec<u64> = tiles
        .iter()
        .scan(0u64, |at, tile| {
            let (tw, th) = box_size(&tile.target);
            let here = *at;
            *at += padded_row(tw) as u64 * th as u64;
            Some(here)
        })
        .collect();
    let total = tiles.last().map_or(0, |t| {
        let (tw, th) = box_size(&t.target);
        offsets[offsets.len() - 1] + padded_row(tw) as u64 * th as u64
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retouch_staging"),
        size: total,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // Stands in for the blurred match in a clone, which never reads it.
    let blank = texture(device, "retouch_blank", 1, 1, wgpu::TextureUsages::TEXTURE_BINDING);

    let mut encoder = device.create_command_encoder(&Default::default());
    let mut kept = Vec::new();
    for (tile, &at) in tiles.iter().zip(&offsets) {
        kept.extend(encode_tile(device, queue, passes, source, stroke, tile, &mut encoder, &blank, &staging, at));
    }
    queue.submit(std::iter::once(encoder.finish()));

    let (tx, rx) = std::sync::mpsc::channel();
    staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::MaintainBase::Wait);
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("GPU buffer map failed: {e}"))?;
    drop(kept);

    let mapped = staging.slice(..).get_mapped_range();
    let out = tiles
        .iter()
        .zip(&offsets)
        .map(|(tile, &at)| {
            let (tw, th) = box_size(&tile.target);
            let (row, padded) = (tw as usize * LINEAR_F32_BYTES_PER_PIXEL, padded_row(tw) as usize);
            let mut data = Vec::with_capacity(row * th as usize);
            for y in 0..th as usize {
                let from = at as usize + y * padded;
                data.extend_from_slice(&mapped[from..from + row]);
            }
            data
        })
        .collect();
    drop(mapped);
    staging.unmap();
    Ok(out)
}

/// One stroke laid over `source`, every tile rendered against the image as it
/// was before the stroke.
fn apply_stroke(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &RetouchPasses,
    source: &mut LinearSource,
    stroke: &RetouchPayload,
) -> Result<(), String> {
    let (width, height) = (source.width, source.height);
    let placed = place_retouch(stroke, width, height);
    let tiles = retouch_tiles(&placed, width, height);
    let mut rendered = Vec::with_capacity(tiles.len());
    let mut start = 0;
    while start < tiles.len() {
        let mut end = start + 1;
        let mut bytes = tile_bytes(&placed, &tiles[start], width, height);
        while end < tiles.len() {
            bytes += tile_bytes(&placed, &tiles[end], width, height);
            if bytes > BATCH_BYTES {
                break;
            }
            end += 1;
        }
        rendered.extend(render_batch(device, queue, passes, source, &placed, &tiles[start..end])?);
        start = end;
    }
    for (tile, data) in tiles.iter().zip(rendered) {
        let [x0, y0, x1, y1] = tile.target;
        source.write(x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32, &data);
    }
    Ok(())
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
    apply_retouch_with(device, queue, &create_passes(device), source, strokes)
}

/// [`apply_retouch`] with pipelines already built for `device`.
pub fn apply_retouch_with(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &RetouchPasses,
    source: &mut LinearSource,
    strokes: &[RetouchPayload],
) -> Result<(), String> {
    for stroke in strokes.iter().filter(|s| s.is_valid()) {
        apply_stroke(device, queue, passes, source, stroke)?;
    }
    Ok(())
}

// ── Following the strokes as they change ──────────────────────────────────────

/// Bytes of overwritten blocks a [`StrokeJournal`] keeps; past it the oldest
/// strokes can no longer be rolled back.
pub const MAX_JOURNAL_BYTES: usize = 256 << 20;

/// The strokes a [`LinearSource`]'s blocks hold, with what each one wrote
/// over, so a list sharing only their start is reached by rolling back.
pub struct StrokeJournal {
    strokes: Vec<RetouchPayload>,
    // What each stroke from `floor` on overwrote; earlier ones were let go to stay under the cap.
    entries: VecDeque<BlockJournal>,
    floor: usize,
    bytes: usize,
    cap: usize,
}

fn journal_bytes(journal: &BlockJournal) -> usize {
    journal.values().map(|held| held.as_ref().map_or(0, Vec::len)).sum()
}

impl Default for StrokeJournal {
    fn default() -> Self {
        Self::new()
    }
}

impl StrokeJournal {
    pub fn new() -> Self {
        Self::with_cap(MAX_JOURNAL_BYTES)
    }

    pub fn with_cap(cap: usize) -> Self {
        Self { strokes: Vec::new(), entries: VecDeque::new(), floor: 0, bytes: 0, cap }
    }

    pub fn strokes(&self) -> &[RetouchPayload] {
        &self.strokes
    }

    pub fn cap(&self) -> usize {
        self.cap
    }

    fn push(&mut self, stroke: RetouchPayload, journal: BlockJournal) {
        self.strokes.push(stroke);
        self.bytes += journal_bytes(&journal);
        self.entries.push_back(journal);
        while self.bytes > self.cap {
            let Some(oldest) = self.entries.pop_front() else { break };
            self.bytes -= journal_bytes(&oldest);
            self.floor += 1;
        }
    }

    fn pop(&mut self) -> BlockJournal {
        self.strokes.pop();
        let journal = self.entries.pop_back().unwrap_or_default();
        self.bytes -= journal_bytes(&journal);
        journal
    }
}

/// How [`follow_strokes`] reached the strokes asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Followed {
    /// By rolling back and applying only what differed; `touched` saw every block that changed.
    Stepped,
    /// From the untouched image, since what differed had been rolled past the cap.
    Restarted,
}

/// Brings `source`, whose blocks hold `journal`'s strokes, in line with
/// `strokes`: the strokes after the start the two share are rolled back, then
/// the rest are applied. `touched` is given each block that changes, with what
/// it held just before, at least once and first with what it held on entry.
pub fn follow_strokes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    passes: &RetouchPasses,
    source: &mut LinearSource,
    journal: &mut StrokeJournal,
    strokes: &[RetouchPayload],
    mut touched: impl FnMut((u32, u32), Option<&[u8]>),
) -> Result<Followed, String> {
    let shared = journal.strokes.iter().zip(strokes).take_while(|(a, b)| a == b).count();
    let followed = if shared < journal.floor {
        source.clear_blocks();
        *journal = StrokeJournal::with_cap(journal.cap);
        Followed::Restarted
    } else {
        while journal.strokes.len() > shared {
            let undo = journal.pop();
            for key in undo.keys() {
                touched(*key, source.block(*key));
            }
            source.roll_back(undo);
        }
        Followed::Stepped
    };

    for stroke in &strokes[journal.strokes.len()..] {
        source.start_journal();
        let applied = if stroke.is_valid() { apply_stroke(device, queue, passes, source, stroke) } else { Ok(()) };
        let undo = source.take_journal();
        if let Err(e) = applied {
            source.roll_back(undo);
            return Err(e);
        }
        for (key, held) in &undo {
            touched(*key, held.as_deref());
        }
        journal.push(stroke.clone(), undo);
    }
    Ok(followed)
}
