// src-native/src/gpu/histogram.rs
//
// Bins the overview through the develop chain and draws the curves, without
// the bins ever reaching the CPU. Mirrors histogramPipeline.ts; the draws land
// in textures egui shows rather than in canvases.

use wgpu::ShaderStages as S;

use super::calc_params::CalcParams;
use super::frame_timer::{FrameTimer, PassTiming};
use super::source::SourceImage;
use super::{bind, stage, Gpu, CANVAS_FORMAT};
use crate::export_rendering::{output_space_to_bytes, SlidersPayload, ToneMapPayload, OUTPUT_SPACE_BYTES};
use crate::icc::Matrix3;
use crate::model::crop::{crop_view, crop_view_floats, Crop, CROP_VIEW_BYTES, EVERYWHERE, FULL_FRAME};
use crate::model::curves::{CHANNELS, COLOURS};
use crate::output_space::OutputSpace;
use crate::shaders::{HISTOGRAM_DRAW_WGSL, HISTOGRAM_SCALE_WGSL, HISTOGRAM_WGSL};

const BINS_BYTES: u64 = 256 * 3 * 4;

/// A texture egui shows, drawn by the GPU at the size it is shown.
pub struct Plot {
    pub texture: Option<wgpu::Texture>,
    pub id: Option<egui::TextureId>,
}

impl Plot {
    pub fn new() -> Self {
        Self { texture: None, id: None }
    }

    /// The texture at `size`, recreated and re-registered when that changed.
    pub fn sized(&mut self, gpu: &Gpu, size: (u32, u32)) -> (&wgpu::Texture, bool) {
        let size = (size.0.max(1), size.1.max(1));
        let stale = self.texture.as_ref().is_none_or(|t| (t.width(), t.height()) != size);
        if stale {
            let t = gpu.canvas_texture("Plot", size.0, size.1);
            self.id = Some(gpu.register_canvas(&t, self.id, wgpu::FilterMode::Linear));
            self.texture = Some(t);
        }
        (self.texture.as_ref().unwrap(), stale)
    }
}

pub struct Histogram {
    calc: CalcParams,
    output_space: wgpu::Buffer,
    crop_buffer: wgpu::Buffer,
    crop: Crop,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    bins: wgpu::Buffer,
    scale_pipeline: wgpu::ComputePipeline,
    scale_group: wgpu::BindGroup,
    draw: wgpu::RenderPipeline,
    backdrops: Vec<wgpu::RenderPipeline>,
    draw_group: wgpu::BindGroup,
    group: Option<wgpu::BindGroup>,
    source: Option<SourceImage>,
    pub timer: FrameTimer,
}

impl Histogram {
    pub fn new(gpu: &Gpu) -> Self {
        let c = S::COMPUTE;
        let calc = CalcParams::new(gpu);
        let output_space = gpu.uniform("Histogram Output Space", OUTPUT_SPACE_BYTES);
        let crop_buffer = gpu.uniform("Histogram Crop View", CROP_VIEW_BYTES as u64);
        let layout = stage::layout(
            gpu,
            "Histogram",
            &[
                bind::texture(0, c, true),
                bind::storage(1, c, true),
                bind::storage(2, c, false),
                bind::uniform(3, c),
                bind::uniform(4, c),
            ],
        );
        let pipeline = stage::compute_pipeline(gpu, "Histogram Compute Pipeline", HISTOGRAM_WGSL, &layout);
        let bins = gpu.storage("Histogram Bins", BINS_BYTES, wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC);
        let scale = gpu.storage("Histogram Scale", 4, wgpu::BufferUsages::empty());
        let scale_layout = stage::layout(gpu, "Histogram Scale", &[bind::storage(0, c, true), bind::storage(1, c, false)]);
        let scale_pipeline = stage::compute_pipeline(gpu, "Histogram Scale Pipeline", HISTOGRAM_SCALE_WGSL, &scale_layout);
        let scale_group = stage::bind_group(gpu, "Histogram Scale", &scale_layout, &[bins.as_entire_binding(), scale.as_entire_binding()]);

        let f = S::FRAGMENT;
        let draw_layout = stage::layout(gpu, "Histogram Draw", &[bind::storage(0, f, true), bind::storage(1, f, true)]);
        let module = stage::shader(gpu, "Histogram Draw Shader", HISTOGRAM_DRAW_WGSL);
        let draw = stage::render_pipeline(gpu, "Histogram Draw", &module, &draw_layout, "fs_main", CANVAS_FORMAT, None, &[]);
        // The canvas is sRGB and unorm, so the tint is written as its code values.
        let backdrops = (0..CHANNELS)
            .map(|channel| {
                let [r, g, b] = COLOURS[channel].map(|v| v as f64 / 255.0);
                let index = if channel == 0 { 3.0 } else { channel as f64 - 1.0 };
                let constants = [("CHANNEL", index), ("TINT_R", r), ("TINT_G", g), ("TINT_B", b)];
                stage::render_pipeline(gpu, "Histogram Backdrop", &module, &draw_layout, "fs_main", CANVAS_FORMAT, None, &constants)
            })
            .collect();
        let draw_group = stage::bind_group(gpu, "Histogram Draw", &draw_layout, &[bins.as_entire_binding(), scale.as_entire_binding()]);

        let h = Self {
            calc,
            output_space,
            crop_buffer,
            crop: FULL_FRAME,
            layout,
            pipeline,
            bins,
            scale_pipeline,
            scale_group,
            draw,
            backdrops,
            draw_group,
            group: None,
            source: None,
            timer: FrameTimer::new(gpu),
        };
        // An unwritten uniform is a zero matrix, which would bin a black image.
        h.set_output_space(gpu, gpu.display_space, None);
        h
    }

    pub fn set_source(&mut self, gpu: &Gpu, image: SourceImage, src_to_working: &Matrix3) {
        self.calc.update_source_space(src_to_working);
        let view = image.texture.create_view(&Default::default());
        self.group = Some(stage::bind_group(
            gpu,
            "Histogram",
            &self.layout,
            &[
                wgpu::BindingResource::TextureView(&view),
                self.calc.params.as_entire_binding(),
                self.bins.as_entire_binding(),
                self.output_space.as_entire_binding(),
                self.crop_buffer.as_entire_binding(),
            ],
        ));
        self.source = Some(image);
    }

    pub fn clear(&mut self) {
        self.source = None;
        self.group = None;
    }

    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    pub fn source(&self) -> Option<&SourceImage> {
        self.source.as_ref()
    }

    pub fn set_output_space(&self, gpu: &Gpu, destination: OutputSpace, proof: Option<OutputSpace>) {
        gpu.queue.write_buffer(&self.output_space, 0, &output_space_to_bytes(destination, proof));
    }

    pub fn set_tone_map(&self, t: &ToneMapPayload) {
        self.calc.update_tone_map(t);
    }

    pub fn set_curves(&self, lut: Option<&[f32]>) {
        self.calc.update_curves(lut);
    }

    pub fn set_crop(&mut self, crop: Crop) {
        self.crop = crop;
    }

    fn draw_into(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture, pipeline: &wgpu::RenderPipeline, timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>) {
        let view = target.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Histogram Draw Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.draw_group, &[]);
        pass.draw(0..6, 0..1);
    }

    /// Bins the image, reduces the scale and draws the curves, all in one
    /// submit. The backdrop gets one curve channel's bins in that curve's colour.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        sliders: &SlidersPayload,
        target: Option<&wgpu::Texture>,
        backdrop: Option<(&wgpu::Texture, usize)>,
        measure: bool,
        done: impl FnOnce(Vec<PassTiming>, f64) + Send + 'static,
    ) {
        let (Some(group), Some(source)) = (&self.group, &self.source) else { return };
        let start = std::time::Instant::now();
        self.calc.update_sliders(sliders);
        let view = crop_view(&self.crop, source.width, source.height);
        let crop = crop_view_floats(
            view.origin,
            view.axis_x,
            view.axis_y,
            [view.width as f64, view.height as f64],
            EVERYWHERE,
            EVERYWHERE,
            EVERYWHERE,
        );
        gpu.queue.write_buffer(&self.crop_buffer, 0, &crop);

        if measure {
            self.timer.begin_frame();
        }
        let slots = if measure {
            [self.timer.claim("bin"), self.timer.claim("scale"), self.timer.claim("draw")]
        } else {
            [None; 3]
        };
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Histogram") });
        self.calc.record(&mut encoder, None);
        encoder.clear_buffer(&self.bins, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Histogram Pass"),
                timestamp_writes: self.timer.compute_writes(slots[0]),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(source.width.div_ceil(16), source.height.div_ceil(16), 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Histogram Scale Pass"),
                timestamp_writes: self.timer.compute_writes(slots[1]),
            });
            pass.set_pipeline(&self.scale_pipeline);
            pass.set_bind_group(0, &self.scale_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        if let Some(target) = target {
            self.draw_into(&mut encoder, target, &self.draw, self.timer.render_writes(slots[2]));
        }
        if let Some((target, channel)) = backdrop {
            self.draw_into(&mut encoder, target, &self.backdrops[channel], None);
        }
        if measure {
            self.timer.resolve(&mut encoder);
        }
        gpu.queue.submit(Some(encoder.finish()));
        let record_ms = start.elapsed().as_secs_f64() * 1000.0;
        if measure {
            self.timer.read(move |passes| done(passes, record_ms));
        }
    }
}
