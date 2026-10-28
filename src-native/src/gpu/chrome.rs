// src-native/src/gpu/chrome.rs
//
// The CSS the window chrome is made of, drawn by chrome.wgsl from inside
// egui's own render pass, so a panel's glass, its shadows and the text on it
// stay in paint order. Each frame collects its primitives here; each painter
// call becomes one instanced draw over a range of them.

use bytemuck::{Pod, Zeroable};
use egui::{Color32, Rect};
use wgpu::ShaderStages as S;

use super::{bind, stage, Gpu, CANVAS_FORMAT};
use crate::shaders::{BLUR_WGSL, CHROME_WGSL};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default, Debug)]
pub struct Prim {
    pub bounds: [f32; 4],
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub color: [f32; 4],
    pub color2: [f32; 4],
    pub p0: [f32; 4],
    pub p1: [f32; 4],
    pub mode: u32,
    pub _pad: [u32; 3],
}

pub mod mode {
    pub const FILL: u32 = 0;
    pub const LINEAR: u32 = 1;
    pub const SHADOW: u32 = 2;
    pub const INSET: u32 = 3;
    pub const GLASS: u32 = 4;
    pub const SCRIM: u32 = 5;
    pub const RING: u32 = 6;
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    screen: [f32; 2],
    ppp: f32,
    scrim: f32,
    saturate: f32,
    brightness: f32,
    _pad: [f32; 2],
}

/// Premultiplied, as egui and the shader blend.
pub fn premul(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_array();
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0]
}

/// The material's lighting, as glass.css exposes it to each surface.
#[derive(Clone, Copy, Debug)]
pub struct GlassStyle {
    pub angle: f32,
    pub rim: [f32; 4],
    pub gloss: [f32; 2],
    pub tint: Color32,
    /// Whether the surface frosts what is behind it, or only tints.
    pub backdrop: bool,
    /// How much of the backdrop shows, below 1 under a dimming scrim.
    pub backdrop_level: f32,
}

impl GlassStyle {
    /// The side panels, toolbar and title bar chips.
    pub fn panel(tint: Color32) -> Self {
        Self { angle: 142.0, rim: [0.22, 0.1, 0.07, 0.12], gloss: [0.0, 0.0], tint, backdrop: true, backdrop_level: 1.0 }
    }

    /// The toggles' thumb and the glass buttons' face: no blur, a brighter rim
    /// and the gloss the big panels leave out.
    pub fn chip() -> Self {
        Self {
            angle: 137.0,
            rim: [0.5, 0.14, 0.05, 0.12],
            gloss: [0.3, 0.1],
            tint: Color32::from_white_alpha((0.07 * 255.0) as u8),
            backdrop: false,
            backdrop_level: 1.0,
        }
    }
}

/// The primitives of one frame, in physical pixels.
pub struct Fx {
    pub ppp: f32,
    pub prims: Vec<Prim>,
    /// Applied to everything pushed while it is set, as CSS opacity on a group.
    pub opacity: f32,
}

pub struct Shadow {
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub color: Color32,
}

pub const fn shadow(x: f32, y: f32, blur: f32, spread: f32, color: Color32) -> Shadow {
    Shadow { offset: [x, y], blur, spread, color }
}

impl Fx {
    pub fn new(ppp: f32) -> Self {
        Self { ppp, prims: Vec::with_capacity(256), opacity: 1.0 }
    }

    fn px(&self, r: Rect) -> [f32; 4] {
        [r.min.x * self.ppp, r.min.y * self.ppp, r.max.x * self.ppp, r.max.y * self.ppp]
    }

    fn radii(&self, r: [f32; 4]) -> [f32; 4] {
        r.map(|v| v * self.ppp)
    }

    fn grown(b: [f32; 4], by: f32) -> [f32; 4] {
        [b[0] - by, b[1] - by, b[2] + by, b[3] + by]
    }

    fn push(&mut self, mut p: Prim) -> usize {
        let o = self.opacity;
        if o < 1.0 {
            p.color = p.color.map(|c| c * o);
            if p.mode == mode::GLASS {
                p.color2[1] *= o;
            } else {
                p.color2 = p.color2.map(|c| c * o);
            }
        }
        self.prims.push(p);
        self.prims.len() - 1
    }

    fn base(&self, rect: Rect, radii: [f32; 4], mode: u32) -> Prim {
        let r = self.px(rect);
        Prim { bounds: Self::grown(r, 1.0), rect: r, radii: self.radii(radii), mode, ..Default::default() }
    }

    pub fn fill(&mut self, rect: Rect, radii: [f32; 4], color: Color32) -> usize {
        let mut p = self.base(rect, radii, mode::FILL);
        p.color = premul(color);
        self.push(p)
    }

    /// A gradient from `from` at point `a` to `to` at point `b`, in points.
    pub fn linear(&mut self, rect: Rect, radii: [f32; 4], a: egui::Pos2, b: egui::Pos2, from: Color32, to: Color32) -> usize {
        let mut p = self.base(rect, radii, mode::LINEAR);
        p.color = premul(from);
        p.color2 = premul(to);
        p.p0 = [a.x * self.ppp, a.y * self.ppp, b.x * self.ppp, b.y * self.ppp];
        self.push(p)
    }

    pub fn ring(&mut self, rect: Rect, radii: [f32; 4], width: f32, color: Color32) -> usize {
        let mut p = self.base(rect, radii, mode::RING);
        p.color = premul(color);
        p.p0 = [width * self.ppp, 0.0, 0.0, 0.0];
        self.push(p)
    }

    /// An outer box shadow, cut out under the element as CSS does.
    pub fn shadow(&mut self, rect: Rect, radius: f32, s: &Shadow) -> usize {
        let mut p = self.base(rect, [radius; 4], mode::SHADOW);
        let reach = (s.blur * 1.5 + s.spread.max(0.0) + s.offset[0].abs().max(s.offset[1].abs())) * self.ppp;
        p.bounds = Self::grown(p.rect, reach + 2.0);
        p.color = premul(s.color);
        p.p0 = [s.offset[0] * self.ppp, s.offset[1] * self.ppp, s.blur * self.ppp, s.spread * self.ppp];
        self.push(p)
    }

    pub fn inset(&mut self, rect: Rect, radius: f32, s: &Shadow) -> usize {
        let mut p = self.base(rect, [radius; 4], mode::INSET);
        p.color = premul(s.color);
        p.p0 = [s.offset[0] * self.ppp, s.offset[1] * self.ppp, s.blur * self.ppp, s.spread * self.ppp];
        self.push(p)
    }

    pub fn glass(&mut self, rect: Rect, radii: [f32; 4], g: &GlassStyle) -> usize {
        let mut p = self.base(rect, radii, mode::GLASS);
        p.color = premul(g.tint);
        p.color2 = [g.backdrop_level, 1.0, 0.0, 0.0];
        p.p0 = [g.angle, g.rim[0], g.rim[1], g.rim[2]];
        p.p1 = [g.rim[3], if g.backdrop { 1.0 } else { 0.0 }, g.gloss[0], g.gloss[1]];
        self.push(p)
    }

    pub fn scrim(&mut self, screen: Rect) -> usize {
        let mut p = self.base(screen, [0.0; 4], mode::SCRIM);
        p.bounds = p.rect;
        self.push(p)
    }

    pub fn mark(&self) -> usize {
        self.prims.len()
    }

    /// Hand everything pushed since `from` to the painter, in its paint order.
    pub fn flush(&self, painter: &egui::Painter, from: usize) {
        self.paint(painter, from..self.prims.len());
    }

    /// Hand the primitives `range` to the painter, in its paint order.
    pub fn paint(&self, painter: &egui::Painter, range: std::ops::Range<usize>) {
        if range.is_empty() {
            return;
        }
        // The viewport covers the whole screen so the shader's pixel
        // coordinates are the window's; the painter's clip still scissors.
        let callback = egui_wgpu::Callback::new_paint_callback(
            painter.ctx().screen_rect(),
            Draw { range: range.start as u32..range.end as u32 },
        );
        painter.add(callback);
    }
}

struct Draw {
    range: std::ops::Range<u32>,
}

impl egui_wgpu::CallbackTrait for Draw {
    fn paint(&self, info: egui::PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        let Some(r) = resources.get::<ChromeResources>() else { return };
        if self.range.end as u64 > r.capacity {
            return;
        }
        let clip = info.clip_rect_in_pixels();
        if clip.width_px <= 0 || clip.height_px <= 0 {
            return;
        }
        pass.set_viewport(0.0, 0.0, info.screen_size_px[0] as f32, info.screen_size_px[1] as f32, 0.0, 1.0);
        pass.set_pipeline(&r.pipeline);
        pass.set_bind_group(0, &r.bind_group, &[]);
        pass.set_vertex_buffer(0, r.instances.slice(..));
        pass.draw(0..4, self.range.clone());
    }
}

/// What the callbacks draw with, kept in egui's callback resources.
pub struct ChromeResources {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: u64,
    sampler: wgpu::Sampler,
}

/// The frosted copy of the window behind the panels.
pub struct Blur {
    compose: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: [wgpu::Buffer; 4],
    textures: Option<[wgpu::Texture; 3]>,
    size: (u32, u32),
    empty: wgpu::Texture,
}

impl Blur {
    pub fn new(gpu: &Gpu) -> Self {
        let f = S::FRAGMENT;
        let layout = stage::layout(gpu, "Blur", &[bind::texture(0, f, true), bind::sampler(1, f, true), bind::uniform(2, f)]);
        let module = stage::shader(gpu, "Blur", BLUR_WGSL);
        let pipeline = |entry| stage::render_pipeline(gpu, entry, &module, &layout, entry, CANVAS_FORMAT, None, &[]);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Blur Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            compose: pipeline("compose"),
            downsample: pipeline("downsample"),
            blur_h: pipeline("blur_h"),
            blur_v: pipeline("blur_v"),
            layout,
            sampler,
            params: std::array::from_fn(|_| gpu.uniform("Blur Params", 48)),
            textures: None,
            size: (0, 0),
            empty: gpu.canvas_texture("Empty Canvas", 1, 1),
        }
    }

    /// The blurred texture, at a quarter of the window.
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.textures.as_ref().map(|t| &t[1])
    }

    /// Frost the window behind the panels: `canvas` over `backdrop`, under the
    /// scrim, blurred by the CSS 16px.
    pub fn run(&mut self, gpu: &Gpu, canvas: Option<&wgpu::Texture>, screen: (u32, u32), ppp: f32, backdrop: [f32; 3], scrim: f32) -> bool {
        let half = ((screen.0 / 2).max(1), (screen.1 / 2).max(1));
        let quarter = ((screen.0 / 4).max(1), (screen.1 / 4).max(1));
        let mut replaced = false;
        if self.size != screen || self.textures.is_none() {
            let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
            self.textures = Some([
                gpu.texture("Blur Half", half.0, half.1, CANVAS_FORMAT, usage),
                gpu.texture("Blur Quarter", quarter.0, quarter.1, CANVAS_FORMAT, usage),
                gpu.texture("Blur Quarter H", quarter.0, quarter.1, CANVAS_FORMAT, usage),
            ]);
            self.size = screen;
            replaced = true;
        }
        let [t_half, t_quarter, t_h] = self.textures.as_ref().unwrap();
        let canvas = canvas.unwrap_or(&self.empty);
        let sigma = 16.0 * ppp / 4.0;
        let write = |i: usize, target: (u32, u32), source: &wgpu::Texture, sigma: f32| {
            let mut floats = [0f32; 12];
            floats[..3].copy_from_slice(&backdrop);
            floats[3] = 1.0;
            floats[4] = 1.0 / source.width() as f32;
            floats[5] = 1.0 / source.height() as f32;
            floats[6] = sigma;
            floats[7] = ppp;
            floats[8] = scrim;
            floats[9] = target.0 as f32;
            floats[10] = target.1 as f32;
            gpu.queue.write_buffer(&self.params[i], 0, bytemuck::cast_slice(&floats));
        };
        let passes: [(&wgpu::RenderPipeline, &wgpu::Texture, &wgpu::Texture, (u32, u32), f32); 4] = [
            (&self.compose, canvas, t_half, half, 0.0),
            (&self.downsample, t_half, t_quarter, quarter, 0.0),
            (&self.blur_h, t_quarter, t_h, quarter, sigma),
            (&self.blur_v, t_h, t_quarter, quarter, sigma),
        ];
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Frost") });
        for (i, (pipeline, source, target, size, sigma)) in passes.into_iter().enumerate() {
            write(i, size, source, sigma);
            let source_view = source.create_view(&Default::default());
            let group = stage::bind_group(
                gpu,
                "Blur",
                &self.layout,
                &[wgpu::BindingResource::TextureView(&source_view), wgpu::BindingResource::Sampler(&self.sampler), self.params[i].as_entire_binding()],
            );
            let target_view = target.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frost Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit(Some(encoder.finish()));
        replaced
    }
}

impl ChromeResources {
    pub fn install(gpu: &Gpu, blur: &Blur) {
        let f = S::FRAGMENT | S::VERTEX;
        let layout = stage::layout(gpu, "Chrome", &[bind::uniform(0, f), bind::texture(1, f, true), bind::sampler(2, f, true)]);
        let module = stage::shader(gpu, "Chrome", CHROME_WGSL);
        let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Chrome"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4,
            4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Uint32
        ];
        let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Chrome"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Prim>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let globals = gpu.uniform("Chrome Globals", std::mem::size_of::<Globals>() as u64);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Chrome Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let capacity = 256;
        let instances = Self::instance_buffer(gpu, capacity);
        let bind_group = Self::group(gpu, &layout, &globals, blur.output().unwrap_or(&blur.empty), &sampler);
        let resources = Self { pipeline, layout, bind_group, globals, instances, capacity, sampler };
        gpu.renderer.write().callback_resources.insert(resources);
    }

    fn instance_buffer(gpu: &Gpu, capacity: u64) -> wgpu::Buffer {
        gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Chrome Instances"),
            size: capacity * std::mem::size_of::<Prim>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn group(gpu: &Gpu, layout: &wgpu::BindGroupLayout, globals: &wgpu::Buffer, backdrop: &wgpu::Texture, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
        let view = backdrop.create_view(&Default::default());
        stage::bind_group(
            gpu,
            "Chrome",
            layout,
            &[globals.as_entire_binding(), wgpu::BindingResource::TextureView(&view), wgpu::BindingResource::Sampler(sampler)],
        )
    }

    /// Upload one frame's primitives, ahead of egui drawing them.
    pub fn upload(gpu: &Gpu, fx: &Fx, screen_px: [f32; 2], scrim: f32, blur: Option<&Blur>) {
        let mut renderer = gpu.renderer.write();
        let Some(r) = renderer.callback_resources.get_mut::<ChromeResources>() else { return };
        if let Some(blur) = blur {
            r.bind_group = Self::group(gpu, &r.layout, &r.globals, blur.output().unwrap_or(&blur.empty), &r.sampler);
        }
        let globals = Globals { screen: screen_px, ppp: fx.ppp, scrim, saturate: 0.8, brightness: 1.08, _pad: [0.0; 2] };
        gpu.queue.write_buffer(&r.globals, 0, bytemuck::bytes_of(&globals));
        let needed = fx.prims.len() as u64;
        if needed > r.capacity {
            r.capacity = needed.next_power_of_two();
            r.instances = Self::instance_buffer(gpu, r.capacity);
        }
        if !fx.prims.is_empty() {
            gpu.queue.write_buffer(&r.instances, 0, bytemuck::cast_slice(&fx.prims));
        }
    }
}
