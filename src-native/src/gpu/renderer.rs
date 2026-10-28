// src-native/src/gpu/renderer.rs
//
// The preview's render chain: a sharp region at the resolution the zoom needs,
// over a low-resolution backdrop of the whole image, encoded into a canvas the
// size of the window. A port of renderer.ts, decision for decision; only the
// canvas differs, since it is a texture egui draws rather than a swap chain.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wgpu::ShaderStages as S;

use super::calc_params::CalcParams;
use super::frame_timer::{FrameTimer, PassTiming};
use super::source::SourceImage;
use super::stage::{self, full_screen_pass, PassOptions, RenderStage, StageDesc, PREMULTIPLIED_OVER};
use super::{bind, Gpu, CANVAS_FORMAT, SHARP_FORMAT, VIEW_FORMAT, WORKING_FORMAT};
use crate::export_rendering::{output_space_to_bytes, SlidersPayload, ToneMapPayload, OUTPUT_SPACE_BYTES};
use crate::icc::Matrix3;
use crate::model::crop::{crop_view_floats, CROP_VIEW_BYTES, EVERYWHERE};
use crate::model::viewport::*;
use crate::output_space::OutputSpace;
use crate::shaders::*;

/// The chain's textures grow in steps of this many pixels.
const TEXTURE_STEP: u32 = 256;
/// The widest apron the blurs can ask for. Must match MAX_RADIUS in calcSharpTexture.wgsl.
pub const MAX_APRON: f64 = 180.0;
/// How long the view has to hold still before a render that was put off runs.
const SETTLE: Duration = Duration::from_millis(120);
/// A sharp render that took less than this is redone even mid-gesture.
const CHEAP_MS: f64 = 12.0;
/// How long after the adjustments change the backdrop catches up, once idle.
const BACKDROP_IDLE: Duration = Duration::from_millis(300);
/// The `ViewSample` uniform in viewSample.wgsl.
const VIEW_SAMPLE_BYTES: u64 = 32;

// Must match the constants in calcParams.wgsl, as sharpnessVisibility.ts does.
const SIGMA_FADE_START: f64 = 0.3;
const TEXTURE_SIGMA_LO: f64 = 1.2;
const TEXTURE_SIGMA_HI: f64 = 5.0;
const MIN_CLARITY_SIGMA: f64 = 20.0;
const MAX_CLARITY_SIGMA: f64 = 60.0;
const CLARITY_SIGMA_FRACTION: f64 = 0.01;
const MIN_SIGMA: f64 = 0.7;

/// Whether the sharpness stage can change a single pixel of this render. Only
/// the predicate is duplicated from calcParams.wgsl, which stays the authority.
pub fn sharpness_is_visible(s: &SlidersPayload, render_scale: f64) -> bool {
    let resolvable = |sigma: f64| sigma * render_scale > SIGMA_FADE_START;
    (s.clarity != 0.0 && resolvable(MIN_CLARITY_SIGMA))
        || (s.texture != 0.0 && resolvable(TEXTURE_SIGMA_LO))
        || (s.usm_amount != 0.0 && resolvable(s.usm_radius as f64))
}

/// How many render pixels the blurs read on either side of a pixel, or 0 when
/// the sharpness stage is skipped. Errs wide.
pub fn sharpness_reach(s: &SlidersPayload, render_scale: f64, full_width: f64, full_height: f64) -> f64 {
    if !sharpness_is_visible(s, render_scale) {
        return 0.0;
    }
    let mut sigma: f64 = 0.0;
    if s.usm_amount != 0.0 {
        sigma = sigma.max(s.usm_radius as f64);
    }
    if s.texture != 0.0 {
        sigma = sigma.max(TEXTURE_SIGMA_HI);
    }
    if s.clarity != 0.0 {
        let clarity = full_width.min(full_height) * CLARITY_SIGMA_FRACTION;
        sigma = sigma.max(clarity.clamp(MIN_CLARITY_SIGMA, MAX_CLARITY_SIGMA));
    }
    MAX_APRON.min((3.0 * (sigma * render_scale).max(2.0 * MIN_SIGMA)).ceil())
}

/// The `ViewSample` uniform: the map, and whether it copies whole texels.
fn view_sample_bytes(a: &Affine) -> [u8; VIEW_SAMPLE_BYTES as usize] {
    let integer = |v: f64| v.fract() == 0.0;
    let unit = |v: f64| v == 0.0 || v.abs() == 1.0;
    let exact = integer(a.origin[0])
        && integer(a.origin[1])
        && [a.axis_x[0], a.axis_x[1], a.axis_y[0], a.axis_y[1]].iter().all(|&v| unit(v));
    let floats = [
        a.origin[0], a.origin[1], a.axis_x[0], a.axis_x[1], a.axis_y[0], a.axis_y[1],
        if exact { 1.0 } else { 0.0 }, 0.0,
    ];
    let mut bytes = [0u8; VIEW_SAMPLE_BYTES as usize];
    for (i, v) in floats.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&(*v as f32).to_le_bytes());
    }
    bytes
}

fn step(n: f64, limit: u32) -> u32 {
    let n = n.max(1.0).ceil() as u32;
    limit.min(n.div_ceil(TEXTURE_STEP) * TEXTURE_STEP)
}

/// One rendered output of the chain, and how to draw it onto the canvas.
struct Held {
    region: ChainRegion,
    texture: wgpu::Texture,
    encode: wgpu::BindGroup,
    encode_over: wgpu::BindGroup,
    crop: wgpu::Buffer,
}

struct Scratch {
    width: u32,
    height: u32,
    view: wgpu::Texture,
    develop: wgpu::Texture,
    sharp_h: Option<wgpu::Texture>,
    bands: Option<wgpu::Texture>,
    view_sample: Option<wgpu::BindGroup>,
    develop_bg: wgpu::BindGroup,
    sharp_h_bg: Option<wgpu::BindGroup>,
    sharp_v_bg: Option<wgpu::BindGroup>,
    composite_bg: wgpu::BindGroup,
}

/// What the preview is holding and drawing, for the F3 panel's preview card.
#[derive(Clone, Debug, Default)]
pub struct PreviewState {
    pub full_width: f64,
    pub full_height: f64,
    pub visible: Option<BoxF>,
    pub scale: f64,
    pub detail: Option<ChainRegion>,
    pub backdrop: Option<ChainRegion>,
    pub backdrop_shown: bool,
    pub source: (u32, u32, u32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PreviewCounters {
    pub detail_renders: u32,
    pub backdrop_renders: u32,
    pub deferred: u32,
    pub last_detail_ms: f64,
}

/// What one rendered frame cost, split by where the time went.
#[derive(Clone, Debug, Default)]
pub struct FrameSample {
    pub interval_ms: f64,
    pub acquire_ms: f64,
    pub record_ms: f64,
    pub turnaround_ms: f64,
    pub passes: Vec<PassTiming>,
}

/// What the renderer wants from the frame loop after a frame.
#[derive(Default)]
pub struct FrameResult {
    /// Draw again after this long, to run a render that was put off.
    pub repaint_after: Option<Duration>,
    /// A frame was drawn, so anything derived from the canvas is stale.
    pub drew: bool,
    /// The canvas texture itself was replaced.
    pub canvas_replaced: bool,
}

pub struct Renderer {
    gpu: Gpu,
    calc: CalcParams,
    view_sample: RenderStage,
    develop: RenderStage,
    sharp_h: RenderStage,
    sharp_v: RenderStage,
    composite: RenderStage,
    encode: RenderStage,
    encode_over: RenderStage,
    sampler: wgpu::Sampler,
    view_sampler: wgpu::Sampler,
    output_space: wgpu::Buffer,
    view_sample_buffer: wgpu::Buffer,

    sliders: SlidersPayload,
    source: Option<SourceImage>,
    frame: Option<ViewFrame>,

    detail: Option<Held>,
    backdrop: Option<Held>,
    detail_stale: bool,
    backdrop_stale: bool,

    moved_at: Instant,
    last_detail_ms: Arc<Mutex<f64>>,
    settle_at: Option<Instant>,
    idle_at: Option<Instant>,
    counters: PreviewCounters,

    dirty: bool,
    frame_changed: bool,
    in_flight: Arc<Mutex<bool>>,

    capacity: (u32, u32),
    canvas_size: (u32, u32),
    fresh_image: bool,
    scratch: Option<Scratch>,

    pub canvas: Option<wgpu::Texture>,

    pub timer: FrameTimer,
    last_frame_start: Option<Instant>,
    pub samples: Arc<Mutex<Vec<FrameSample>>>,
    pub report: Option<PreviewState>,
}

impl Renderer {
    pub fn new(gpu: &Gpu) -> Self {
        let f = S::FRAGMENT;
        let view_sample = stage::create(
            gpu,
            StageDesc::new(
                "View Sample",
                VIEW_SAMPLE_WGSL,
                VIEW_FORMAT,
                &[bind::texture(0, f, true), bind::sampler(1, f, true), bind::uniform(2, f)],
            ),
        );
        let develop = stage::create(
            gpu,
            StageDesc::new(
                "Develop",
                DEVELOP_WGSL,
                WORKING_FORMAT,
                &[bind::texture(0, f, true), bind::sampler(1, f, true), bind::storage(2, f, true)],
            ),
        );
        let blur_bindings = [bind::texture(0, f, false), bind::texture(1, f, false), bind::storage(2, f, true)];
        let sharp_h = stage::create(
            gpu,
            StageDesc { entry_point: "h_main", ..StageDesc::new("Sharp Blur H", CALC_SHARP_TEXTURE_WGSL, SHARP_FORMAT, &blur_bindings) },
        );
        let sharp_v = stage::create(
            gpu,
            StageDesc { entry_point: "v_main", ..StageDesc::new("Sharp Blur V", CALC_SHARP_TEXTURE_WGSL, SHARP_FORMAT, &blur_bindings) },
        );
        let composite = stage::create(gpu, StageDesc::new("Composite", COMPOSITE_WGSL, WORKING_FORMAT, &blur_bindings));
        let encode_bindings = [bind::texture(0, f, false), bind::uniform(1, f), bind::uniform(2, f)];
        let encode = stage::create(gpu, StageDesc::new("Colour Space Encode", COLOR_SPACE_ENCODE_WGSL, CANVAS_FORMAT, &encode_bindings));
        let encode_over = stage::create(
            gpu,
            StageDesc {
                blend: Some(PREMULTIPLIED_OVER),
                ..StageDesc::new("Colour Space Encode Over", COLOR_SPACE_ENCODE_WGSL, CANVAS_FORMAT, &encode_bindings)
            },
        );

        // Develop reads the view texture texel for texel, so nearest copies it
        // exactly wherever the region sits, as the export's sampler does.
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Develop Sampler"),
            ..Default::default()
        });
        // Trilinear, so a source shrunk on screen is read from the mip levels
        // nearest its size rather than aliased from the top.
        let view_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("View Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let output_space = gpu.uniform("Output Space Uniform Buffer", OUTPUT_SPACE_BYTES);
        let view_sample_buffer = gpu.uniform("View Sample Uniform Buffer", VIEW_SAMPLE_BYTES);

        let r = Self {
            gpu: gpu.clone(),
            calc: CalcParams::new(gpu),
            view_sample,
            develop,
            sharp_h,
            sharp_v,
            composite,
            encode,
            encode_over,
            sampler,
            view_sampler,
            output_space,
            view_sample_buffer,
            sliders: SlidersPayload::default(),
            source: None,
            frame: None,
            detail: None,
            backdrop: None,
            detail_stale: true,
            backdrop_stale: true,
            moved_at: Instant::now(),
            last_detail_ms: Arc::new(Mutex::new(0.0)),
            settle_at: None,
            idle_at: None,
            counters: PreviewCounters::default(),
            dirty: false,
            frame_changed: false,
            in_flight: Arc::new(Mutex::new(false)),
            capacity: (0, 0),
            canvas_size: (0, 0),
            fresh_image: true,
            scratch: None,
            canvas: None,
            timer: FrameTimer::new(gpu),
            last_frame_start: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            report: None,
        };
        r.set_output_space(gpu.display_space, None);
        r
    }

    fn create_texture(&self, label: &str, w: u32, h: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
        self.gpu.texture(label, w, h, format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING)
    }

    // Room for the canvas, the pan margin and the widest apron on every side.
    // Grows past that only for a region that needs it, such as a rotated crop.
    fn fit_scratch(&mut self, width: f64, height: f64, sharpening: bool) {
        let limit = self.gpu.max_texture;
        let next = (
            step(width.max(self.capacity.0 as f64), limit),
            step(height.max(self.capacity.1 as f64), limit),
        );
        let resized = next != self.capacity || self.scratch.is_none();
        if resized {
            self.capacity = next;
            let view = self.create_texture("View Sample", next.0, next.1, VIEW_FORMAT);
            let develop = self.create_texture("Developed Image", next.0, next.1, WORKING_FORMAT);
            let had_sharp = self.scratch.as_ref().is_some_and(|s| s.sharp_h.is_some());
            let (sharp_h, bands) = if had_sharp || sharpening {
                (
                    Some(self.create_texture("Sharp Blur H", next.0, next.1, SHARP_FORMAT)),
                    Some(self.create_texture("Detail Bands", next.0, next.1, SHARP_FORMAT)),
                )
            } else {
                (None, None)
            };
            self.scratch = Some(self.bind_scratch(next, view, develop, sharp_h, bands));
        } else if sharpening && self.scratch.as_ref().is_some_and(|s| s.sharp_h.is_none()) {
            let s = self.scratch.take().unwrap();
            let sharp_h = self.create_texture("Sharp Blur H", next.0, next.1, SHARP_FORMAT);
            let bands = self.create_texture("Detail Bands", next.0, next.1, SHARP_FORMAT);
            self.scratch = Some(self.bind_scratch(next, s.view, s.develop, Some(sharp_h), Some(bands)));
        }
    }

    fn bind_scratch(
        &self,
        size: (u32, u32),
        view: wgpu::Texture,
        develop: wgpu::Texture,
        sharp_h: Option<wgpu::Texture>,
        bands: Option<wgpu::Texture>,
    ) -> Scratch {
        use wgpu::BindingResource as R;
        let gpu = &self.gpu;
        let view_view = view.create_view(&Default::default());
        let develop_view = develop.create_view(&Default::default());
        let develop_bg = stage::bind_group(
            gpu,
            "Develop",
            &self.develop.layout,
            &[R::TextureView(&view_view), R::Sampler(&self.sampler), self.calc.params.as_entire_binding()],
        );
        let (mut sharp_h_bg, mut sharp_v_bg) = (None, None);
        let bands_view = bands.as_ref().map(|t| t.create_view(&Default::default()));
        if let (Some(h), Some(_)) = (&sharp_h, &bands) {
            // The horizontal pass never reads binding 1; it is the same developed
            // texture it is already blurring, so the layout can stay shared.
            sharp_h_bg = Some(stage::bind_group(
                gpu,
                "Sharp Blur H",
                &self.sharp_h.layout,
                &[R::TextureView(&develop_view), R::TextureView(&develop_view), self.calc.sharp.as_entire_binding()],
            ));
            let h_view = h.create_view(&Default::default());
            sharp_v_bg = Some(stage::bind_group(
                gpu,
                "Sharp Blur V",
                &self.sharp_v.layout,
                &[R::TextureView(&h_view), R::TextureView(&develop_view), self.calc.sharp.as_entire_binding()],
            ));
        }
        let composite_bg = stage::bind_group(
            gpu,
            "Composite",
            &self.composite.layout,
            &[
                R::TextureView(&develop_view),
                R::TextureView(bands_view.as_ref().unwrap_or(&develop_view)),
                self.calc.sharp.as_entire_binding(),
            ],
        );
        let view_sample = self.source.as_ref().map(|s| self.view_sample_group(s));
        Scratch {
            width: size.0,
            height: size.1,
            view,
            develop,
            sharp_h,
            bands,
            view_sample,
            develop_bg,
            sharp_h_bg,
            sharp_v_bg,
            composite_bg,
        }
    }

    fn view_sample_group(&self, source: &SourceImage) -> wgpu::BindGroup {
        use wgpu::BindingResource as R;
        let view = source.texture.create_view(&Default::default());
        stage::bind_group(
            &self.gpu,
            "View Sample",
            &self.view_sample.layout,
            &[R::TextureView(&view), R::Sampler(&self.view_sampler), self.view_sample_buffer.as_entire_binding()],
        )
    }

    // An output texture for `region`, reusing `held`'s when it is large enough.
    fn output_for(&self, held: Option<Held>, region: ChainRegion, label: &str) -> Held {
        let width = region.bx[2] - region.bx[0];
        let height = region.bx[3] - region.bx[1];
        if let Some(mut h) = held {
            if h.texture.width() as f64 >= width && h.texture.height() as f64 >= height {
                h.region = region;
                return h;
            }
        }
        use wgpu::BindingResource as R;
        let limit = self.gpu.max_texture;
        let texture = self.create_texture(label, step(width, limit), step(height, limit), WORKING_FORMAT);
        let crop = self.gpu.uniform(label, CROP_VIEW_BYTES as u64);
        let view = texture.create_view(&Default::default());
        let resources = [R::TextureView(&view), self.output_space.as_entire_binding(), crop.as_entire_binding()];
        let encode = stage::bind_group(&self.gpu, label, &self.encode.layout, &resources);
        let encode_over = stage::bind_group(&self.gpu, label, &self.encode_over.layout, &resources);
        Held { region, texture, encode, encode_over, crop }
    }

    fn bind_source(&mut self, image: SourceImage) {
        let group = self.view_sample_group(&image);
        if let Some(s) = &mut self.scratch {
            s.view_sample = Some(group);
        }
        self.source = Some(image);
        self.detail_stale = true;
        self.backdrop_stale = true;
    }

    /// A new image, with its source's primaries, which go in together so the
    /// two cannot disagree.
    pub fn load_image(&mut self, image: SourceImage, src_to_working: &Matrix3) {
        self.calc.update_source_space(src_to_working);
        self.bind_source(image);
        self.detail = None;
        self.backdrop = None;
        self.fresh_image = true;
    }

    /// Swaps in a finer copy of the same image, once it has arrived.
    pub fn set_source(&mut self, image: SourceImage) {
        self.bind_source(image);
    }

    pub fn source_size(&self) -> Option<(u32, u32)> {
        self.source.as_ref().map(|s| (s.width, s.height))
    }

    pub fn clear_image(&mut self) {
        self.source = None;
        self.detail = None;
        self.backdrop = None;
        if let Some(s) = &mut self.scratch {
            s.view_sample = None;
        }
        self.request_render();
    }

    /// Ask for a frame if the last `set_frame` moved anything.
    pub fn request_render_if_moved(&mut self) {
        if self.frame_changed {
            self.dirty = true;
        }
    }

    pub fn set_frame(&mut self, next: ViewFrame) {
        self.frame_changed = self.frame != Some(next);
        let moved = match &self.frame {
            None => true,
            Some(f) => next.scale != f.scale || next.to_image.origin != f.to_image.origin || next.to_image.axis_x != f.to_image.axis_x,
        };
        if moved {
            self.moved_at = Instant::now();
        }
        let resized = (next.canvas_width as u32, next.canvas_height as u32) != self.canvas_size;
        let zooming_in = !resized && !self.fresh_image && self.frame.is_some_and(|f| next.scale > f.scale);
        self.frame = Some(next);

        if resized {
            self.canvas_size = (next.canvas_width as u32, next.canvas_height as u32);
            // The backdrop is sized from the canvas.
            self.backdrop_stale = true;
        }
        if !resized && !zooming_in && !self.fresh_image {
            return;
        }

        let reach = 2.0 * (PAN_MARGIN + MAX_APRON + 2.0);
        let shown = if zooming_in {
            Some([0.0, 0.0, next.canvas_width, next.canvas_height])
        } else {
            visible_region(&next).map(|v| v.canvas)
        };
        let width = shown.map_or(0.0, |s| (s[2] - s[0]).ceil()) + reach;
        let height = shown.map_or(0.0, |s| (s[3] - s[1]).ceil()) + reach;
        // Shrinks only once the scratch is well past what is needed, so
        // dragging a window edge does not reallocate on every frame.
        if !zooming_in && 2.0 * width * height < self.capacity.0 as f64 * self.capacity.1 as f64 {
            self.capacity = (0, 0);
            self.scratch = None;
            self.backdrop = None;
            self.backdrop_stale = true;
        }
        let sharpening = self.scratch.as_ref().is_some_and(|s| s.sharp_h.is_some());
        self.fit_scratch(width, height, sharpening);
        self.fresh_image = false;
    }

    fn mark_stale(&mut self) {
        self.detail_stale = true;
        self.backdrop_stale = true;
    }

    /// The destination the preview is encoded for, and the export space it is
    /// being proofed against. Only the encode stage reads it, so what the chain
    /// holds stays good.
    pub fn set_output_space(&self, destination: OutputSpace, proof: Option<OutputSpace>) {
        self.gpu.queue.write_buffer(&self.output_space, 0, &output_space_to_bytes(destination, proof));
    }

    pub fn set_tone_map(&mut self, t: &ToneMapPayload) {
        self.calc.update_tone_map(t);
        self.mark_stale();
    }

    pub fn set_curves(&mut self, lut: Option<&[f32]>) {
        self.calc.update_curves(lut);
        self.mark_stale();
    }

    pub fn set_sliders(&mut self, sliders: SlidersPayload) {
        self.calc.update_sliders(&sliders);
        self.sliders = sliders;
        self.mark_stale();
    }

    /// Ask for a frame. Repeated calls between two frames collapse into one.
    pub fn request_render(&mut self) {
        self.dirty = true;
    }

    pub fn wants_frame(&self) -> bool {
        self.dirty && !*self.in_flight.lock().unwrap()
    }

    /// The frame loop's turn. Draws if a frame was asked for and the previous
    /// one has drained, and runs any render that was put off once it is due.
    pub fn frame(&mut self, measuring: bool, report: bool) -> FrameResult {
        let now = Instant::now();
        let mut result = FrameResult::default();
        for at in [&mut self.settle_at, &mut self.idle_at] {
            if at.is_some_and(|t| now >= t) {
                *at = None;
                self.dirty = true;
            }
        }
        if *self.in_flight.lock().unwrap() {
            result.repaint_after = Some(Duration::from_millis(1));
            return result;
        }
        if self.dirty {
            match self.draw_frame(measuring, report) {
                Some(replaced) => {
                    self.dirty = false;
                    result.drew = true;
                    result.canvas_replaced = replaced;
                }
                None if self.source.is_none() || self.frame.is_none() => self.dirty = false,
                None => {}
            }
        }
        let pending = [self.settle_at, self.idle_at].into_iter().flatten().min();
        result.repaint_after = pending.map(|t| t.saturating_duration_since(now));
        result
    }

    /// Whether the canvas texture was replaced, so egui has to be pointed at
    /// the new one.
    fn ensure_canvas(&mut self, width: u32, height: u32) -> bool {
        if self.canvas.as_ref().is_some_and(|c| c.width() == width && c.height() == height) {
            return false;
        }
        self.canvas = Some(self.gpu.canvas_texture("Preview Canvas", width, height));
        true
    }

    // One run of the chain over `region`, into `output`. Each run writes the
    // view uniforms, so two runs need two submits to keep their own.
    fn record_chain(&mut self, encoder: &mut wgpu::CommandEncoder, view: &ViewFrame, region: &ChainRegion, output: &wgpu::Texture, timed: bool) {
        let (full_width, full_height) = (view.full_width, view.full_height);
        let width = region.bx[2] - region.bx[0];
        let height = region.bx[3] - region.bx[1];
        let sharpening = sharpness_is_visible(&self.sliders, region.scale);
        self.fit_scratch(width, height, sharpening);
        let source = self.source.as_ref().unwrap();
        let map = chain_to_source(region, source.width as f64, source.height as f64, full_width, full_height);
        self.gpu.queue.write_buffer(&self.view_sample_buffer, 0, &view_sample_bytes(&map));
        // Radii are full-resolution measurements, so the blurs need to know how
        // far this render is from full resolution.
        self.calc.update_view(region.scale as f32, full_width as f32, full_height as f32);
        let timing = timed && self.timer.available();
        let mut claim = |label: &str| if timing { self.timer.claim(label) } else { None };
        let slots = [claim("params"), claim("view"), claim("develop")];
        let sharpen_slots = if sharpening { [claim("sharp H"), claim("sharp V")] } else { [None, None] };
        let composite_slot = claim("composite");
        self.calc.record(encoder, self.timer.compute_writes(slots[0]));

        let scissor = Some([0, 0, width as u32, height as u32]);
        let s = self.scratch.as_ref().unwrap();
        let Some(view_sample_bg) = &s.view_sample else { return };
        let views = [
            s.view.create_view(&Default::default()),
            s.develop.create_view(&Default::default()),
            s.sharp_h.as_ref().map(|t| t.create_view(&Default::default())).unwrap_or_else(|| s.view.create_view(&Default::default())),
            s.bands.as_ref().map(|t| t.create_view(&Default::default())).unwrap_or_else(|| s.view.create_view(&Default::default())),
            output.create_view(&Default::default()),
        ];
        let timer = &self.timer;
        let opts = |slot: Option<u32>| PassOptions { scissor, timestamps: timer.render_writes(slot), ..Default::default() };
        // Each stage reads the previous one's target, so they cannot be
        // collapsed: a fragment can only read a texture it is not writing.
        full_screen_pass(encoder, &self.view_sample, &views[0], view_sample_bg, opts(slots[1]));
        full_screen_pass(encoder, &self.develop, &views[1], &s.develop_bg, opts(slots[2]));
        // The blur halves are skipped whenever the effect cannot reach the
        // screen; calcParams zeroes the amount over the same range.
        if sharpening {
            if let (Some(h), Some(v)) = (&s.sharp_h_bg, &s.sharp_v_bg) {
                full_screen_pass(encoder, &self.sharp_h, &views[2], h, opts(sharpen_slots[0]));
                full_screen_pass(encoder, &self.sharp_v, &views[3], v, opts(sharpen_slots[1]));
            }
        }
        full_screen_pass(encoder, &self.composite, &views[4], &s.composite_bg, opts(composite_slot));
    }

    // The whole image at about half the canvas's resolution: it only fills in
    // until the sharp region catches up.
    fn render_backdrop(&mut self, view: &ViewFrame) {
        let source = self.source.as_ref().unwrap();
        let (fw, fh) = (view.full_width, view.full_height);
        let width = (view.canvas_width / 2.0).ceil() + 2.0 * MAX_APRON + 4.0;
        let height = (view.canvas_height / 2.0).ceil() + 2.0 * MAX_APRON + 4.0;
        let fit = backdrop_scale(fw, fh, source.width as f64 / fw, width, height, MAX_APRON);
        let apron = sharpness_reach(&self.sliders, fit, fw, fh);
        let region = plan_region(needed_box([0.0, 0.0, fw, fh], fit), fit, apron, 0.0, fw, fh, width.max(height));
        let previous = self.backdrop.take();
        let held = self.output_for(previous, region, "Backdrop");
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Backdrop") });
        let texture = held.texture.clone();
        self.backdrop = Some(held);
        self.record_chain(&mut encoder, view, &region, &texture, false);
        self.gpu.queue.submit(Some(encoder.finish()));
        self.backdrop_stale = false;
        self.counters.backdrop_renders += 1;
    }

    fn write_crop(&self, held: &Held, view: &ViewFrame, image: BoxF) {
        let a = canvas_to_chain(view, &held.region);
        let bytes = crop_view_floats(
            a.origin,
            a.axis_x,
            a.axis_y,
            [view.canvas_width, view.canvas_height],
            view.keep.unwrap_or(EVERYWHERE),
            image,
            view.clip.unwrap_or(EVERYWHERE),
        );
        self.gpu.queue.write_buffer(&held.crop, 0, &bytes);
    }

    /// Some(changed canvas) once a frame was drawn, None when there was
    /// nothing to draw yet.
    fn draw_frame(&mut self, measuring: bool, report: bool) -> Option<bool> {
        let view = self.frame?;
        self.source.as_ref()?;
        self.scratch.as_ref()?.view_sample.as_ref()?;
        let now = Instant::now();

        let interval_ms = match (measuring, self.last_frame_start) {
            (true, Some(last)) => now.duration_since(last).as_secs_f64() * 1000.0,
            _ => 0.0,
        };
        if measuring {
            self.last_frame_start = Some(now);
            self.timer.begin_frame();
        }

        let (fw, fh) = (view.full_width, view.full_height);
        let visible = visible_region(&view);
        let source = self.source.as_ref().unwrap();
        // Never finer than the source, nor than 1:1: past it the encode stage enlarges.
        let scale = view.scale.min(1.0).min(source.width as f64 / fw);
        let moving = now.duration_since(self.moved_at) < SETTLE;
        let last_detail_ms = *self.last_detail_ms.lock().unwrap();

        // Step 1: decide what the sharp region needs, and put it off while the
        // view is moving and a fresh one would cost more than a frame.
        let mut next: Option<ChainRegion> = None;
        if let Some(v) = &visible {
            let action = detail_action(self.detail.as_ref().map(|d| &d.region), self.detail_stale, v.image, scale, moving, last_detail_ms < CHEAP_MS);
            match action {
                DetailAction::Render => {
                    let needed = needed_box(v.image, scale);
                    let whole = needed_box([0.0, 0.0, fw, fh], scale);
                    let margin = if contains(needed, whole) { 0.0 } else { PAN_MARGIN };
                    let apron = sharpness_reach(&self.sliders, scale, fw, fh);
                    let planned = plan_region(needed, scale, apron, margin, fw, fh, self.gpu.max_texture as f64);
                    let same = self.detail.as_ref().is_some_and(|d| !self.detail_stale && d.region.scale == planned.scale && d.region.bx == planned.bx);
                    if !same {
                        next = Some(planned);
                    }
                }
                DetailAction::Defer => {
                    self.counters.deferred += 1;
                    self.settle_at = Some(now + SETTLE.saturating_sub(now.duration_since(self.moved_at)));
                }
                DetailAction::Keep => {}
            }
        }

        // Step 2: whether the sharp region, as it will be after this frame,
        // covers everything on screen. Wherever it does not, the backdrop does.
        let sharp = next.or(self.detail.as_ref().map(|d| d.region));
        let draw_sharp = match (&sharp, &visible) {
            (Some(s), Some(_)) => shows_detail(s, scale, self.backdrop.as_ref().map(|b| &b.region)),
            _ => false,
        };
        let covered = draw_sharp && contains(valid_box(sharp.as_ref().unwrap()), needed_box(visible.unwrap().image, sharp.unwrap().scale));
        let need_backdrop = visible.is_some() && !covered;
        let mut rendered_backdrop = false;
        if need_backdrop && (self.backdrop_stale || self.backdrop.is_none()) {
            self.render_backdrop(&view);
            rendered_backdrop = true;
        }

        // Step 3: the canvas, and the sharp region's chain, recorded after any
        // backdrop run was submitted so each has its own uniforms.
        let resized = self.ensure_canvas(view.canvas_width as u32, view.canvas_height as u32);
        let acquire_end = Instant::now();
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Render Frame") });

        if let Some(region) = next {
            let previous = self.detail.take();
            let held = self.output_for(previous, region, "Sharp Region");
            let texture = held.texture.clone();
            self.detail = Some(held);
            self.record_chain(&mut encoder, &view, &region, &texture, measuring);
            self.detail_stale = false;
            self.counters.detail_renders += 1;
        }

        // Step 4: the output transform onto the canvas — the backdrop first
        // where it is needed, then the sharp region over it.
        let canvas = self.canvas.as_ref().unwrap().clone();
        let canvas_view = canvas.create_view(&Default::default());
        let drawn = visible.map(|v| pixel_box(v.canvas, canvas.width(), canvas.height()));
        let mut cleared = false;
        if let (Some(drawn), true, Some(backdrop)) = (drawn, need_backdrop, &self.backdrop) {
            self.write_crop(backdrop, &view, image_in_chain(&backdrop.region, fw, fh));
            let slot = if measuring { self.timer.claim("backdrop") } else { None };
            let timestamps = self.timer.render_writes(slot);
            full_screen_pass(
                &mut encoder,
                &self.encode,
                &canvas_view,
                &backdrop.encode,
                PassOptions { clear: Some(wgpu::Color::TRANSPARENT), scissor: Some(drawn), timestamps, ..Default::default() },
            );
            cleared = true;
        }
        if let (Some(drawn), true, Some(detail)) = (drawn, draw_sharp, &self.detail) {
            let image_box = image_in_chain(&detail.region, fw, fh);
            let valid = valid_box(&detail.region);
            let b = detail.region.bx;
            let kept = if covered {
                Some(image_box)
            } else {
                intersect(image_box, [valid[0] - b[0] + 1.0, valid[1] - b[1] + 1.0, valid[2] - b[0] - 1.0, valid[3] - b[1] - 1.0])
            };
            if let Some(kept) = kept {
                self.write_crop(detail, &view, kept);
                let (stage, group) = if cleared { (&self.encode_over, &detail.encode_over) } else { (&self.encode, &detail.encode) };
                let slot = if measuring { self.timer.claim("encode") } else { None };
                let timestamps = self.timer.render_writes(slot);
                full_screen_pass(
                    &mut encoder,
                    stage,
                    &canvas_view,
                    group,
                    PassOptions { clear: Some(wgpu::Color::TRANSPARENT), scissor: Some(drawn), load: cleared, timestamps },
                );
                cleared = true;
            }
        }
        if !cleared {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &canvas_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        if measuring {
            self.timer.resolve(&mut encoder);
        }

        // Step 5: submit, and hold the next frame until this one has drained.
        let submit_at = Instant::now();
        self.gpu.queue.submit(Some(encoder.finish()));
        *self.in_flight.lock().unwrap() = true;
        let in_flight = self.in_flight.clone();
        let timed_detail = next.is_some() && !rendered_backdrop;
        let last = self.last_detail_ms.clone();
        let samples = self.samples.clone();
        let record = measuring.then(|| {
            (interval_ms, acquire_end.duration_since(now).as_secs_f64() * 1000.0, Instant::now().duration_since(acquire_end).as_secs_f64() * 1000.0)
        });
        let passes: Arc<Mutex<Option<Vec<PassTiming>>>> = Arc::new(Mutex::new(None));
        if measuring {
            let slot = passes.clone();
            self.timer.read(move |p| *slot.lock().unwrap() = Some(p));
        }
        self.gpu.queue.on_submitted_work_done(move || {
            let elapsed = submit_at.elapsed().as_secs_f64() * 1000.0;
            if timed_detail {
                *last.lock().unwrap() = elapsed;
            }
            *in_flight.lock().unwrap() = false;
            if let Some((interval_ms, acquire_ms, record_ms)) = record {
                let mut s = samples.lock().unwrap();
                s.push(FrameSample {
                    interval_ms,
                    acquire_ms,
                    record_ms,
                    turnaround_ms: elapsed,
                    passes: passes.lock().unwrap().take().unwrap_or_default(),
                });
                let excess = s.len().saturating_sub(180);
                s.drain(..excess);
            }
        });

        // Once idle, the backdrop catches up with the adjustments, so a later
        // gesture finds it ready.
        if self.backdrop_stale && !rendered_backdrop {
            self.idle_at = Some(now + BACKDROP_IDLE);
            if !moving && next.is_none() && now.duration_since(self.moved_at) > BACKDROP_IDLE {
                self.render_backdrop(&view);
            }
        }

        if report {
            let source = self.source.as_ref().unwrap();
            self.report = Some(PreviewState {
                full_width: fw,
                full_height: fh,
                visible: visible.map(|v| v.image),
                scale: view.scale,
                detail: self.detail.as_ref().map(|d| d.region),
                backdrop: self.backdrop.as_ref().map(|b| b.region),
                backdrop_shown: need_backdrop,
                source: (source.width, source.height, source.texture.mip_level_count()),
            });
        }
        Some(resized)
    }

    pub fn counters(&self) -> PreviewCounters {
        PreviewCounters { last_detail_ms: *self.last_detail_ms.lock().unwrap(), ..self.counters }
    }
}

/// The whole pixels `b` touches, clamped to the target, as a scissor rect.
fn pixel_box(b: BoxF, width: u32, height: u32) -> [u32; 4] {
    let x0 = b[0].floor().max(0.0) as u32;
    let y0 = b[1].floor().max(0.0) as u32;
    let x1 = (b[2].ceil() as u32).min(width);
    let y1 = (b[3].ceil() as u32).min(height);
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}
