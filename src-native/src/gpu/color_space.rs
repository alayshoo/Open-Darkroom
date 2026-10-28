// src-native/src/gpu/color_space.rs
//
// The F3 panel's measurements: the overview binned into a chromaticity grid
// and an Oklab volume, with out-of-gamut tallies. Mirrors colorSpacePipeline.ts
// and gamutOverlay.ts.

use std::sync::{Arc, Mutex};

use wgpu::ShaderStages as S;

use super::calc_params::{matrix_bytes, CalcParams};
use super::frame_timer::{FrameTimer, PassTiming};
use super::source::SourceImage;
use super::{bind, stage, Gpu, CANVAS_FORMAT};
use crate::export_rendering::{output_space_to_bytes, SlidersPayload, ToneMapPayload, OUTPUT_SPACE_BYTES};
use crate::icc::{Matrix3, D65};
use crate::output_space::{column_major, rgb_to_xyz, OutputSpace};
use crate::shaders::{CHROMATICITY_DRAW_WGSL, COLOR_SPACE_SCALE_WGSL, COLOR_SPACE_WGSL, COLOR_VOLUME_DRAW_WGSL};

const XY_TOTAL: u64 = 128 * 128;
const VOL_TOTAL: u64 = 48 * 48 * 48;
const STAT_WORDS: u64 = 8;

/// The spectral locus in CIE 1931 xy, at 10nm from 380nm to 700nm.
const SPECTRAL_LOCUS: [[f32; 2]; 33] = [
    [0.1741, 0.0050], [0.1738, 0.0049], [0.1733, 0.0048], [0.1726, 0.0048],
    [0.1714, 0.0051], [0.1689, 0.0069], [0.1644, 0.0109], [0.1566, 0.0177],
    [0.1440, 0.0297], [0.1241, 0.0578], [0.0913, 0.1327], [0.0454, 0.2950],
    [0.0082, 0.5384], [0.0139, 0.7502], [0.0743, 0.8338], [0.1547, 0.8059],
    [0.2296, 0.7543], [0.3016, 0.6923], [0.3731, 0.6245], [0.4441, 0.5547],
    [0.5125, 0.4866], [0.5752, 0.4242], [0.6270, 0.3725], [0.6658, 0.3340],
    [0.6915, 0.3083], [0.7079, 0.2920], [0.7190, 0.2809], [0.7260, 0.2740],
    [0.7300, 0.2700], [0.7320, 0.2680], [0.7334, 0.2666], [0.7344, 0.2656],
    [0.7347, 0.2653],
];

/// The colour each triangle and its panel swatch are drawn in, in slot order.
pub const GAMUT_TINTS: [u32; 4] = [0x6fd3ff, 0x8ae08a, 0xffc266, 0xff87b3];

const SPACES_OFFSET: usize = 12;
const SPACE_WORDS: usize = 8;
const LOCUS_OFFSET: usize = SPACES_OFFSET + 4 * SPACE_WORDS;
const TRANSFORMED_WORD: usize = LOCUS_OFFSET + 33 * 2;
const VOLUME_WORD: usize = TRANSFORMED_WORD + 1;
const OVERLAY_BYTES: u64 = ((VOLUME_WORD + 1) * 4) as u64;

pub fn space_to_xyz(space: OutputSpace) -> Matrix3 {
    column_major(rgb_to_xyz(space.primaries(), D65))
}

#[derive(Clone, Debug, Default)]
pub struct GamutStats {
    pub counted: u32,
    pub outside: [u32; 4],
}

pub struct ColorSpace {
    calc: CalcParams,
    overlay: wgpu::Buffer,
    output_space: wgpu::Buffer,
    camera: wgpu::Buffer,
    xy_bins: wgpu::Buffer,
    vol_bins: wgpu::Buffer,
    stats: wgpu::Buffer,
    staging: wgpu::Buffer,
    compute_layout: wgpu::BindGroupLayout,
    compute: wgpu::ComputePipeline,
    scale: wgpu::ComputePipeline,
    scale_group: wgpu::BindGroup,
    chromaticity: wgpu::RenderPipeline,
    chromaticity_group: wgpu::BindGroup,
    volume: wgpu::RenderPipeline,
    volume_group: wgpu::BindGroup,
    group: Option<wgpu::BindGroup>,
    source: Option<SourceImage>,
    reading: Arc<Mutex<bool>>,
    pub timer: FrameTimer,
}

impl ColorSpace {
    pub fn new(gpu: &Gpu) -> Self {
        let c = S::COMPUTE;
        let f = S::FRAGMENT;
        let calc = CalcParams::new(gpu);
        let overlay = gpu.storage("Gamut Overlay Buffer", OVERLAY_BYTES, wgpu::BufferUsages::COPY_DST);
        let output_space = gpu.uniform("Colour Space Output Space", OUTPUT_SPACE_BYTES);
        let camera = gpu.uniform("Colour Volume Camera", 16);
        let xy_bins = gpu.storage("Chromaticity Bins", XY_TOTAL * 4, wgpu::BufferUsages::COPY_DST);
        let vol_bins = gpu.storage("Colour Volume Bins", VOL_TOTAL * 4, wgpu::BufferUsages::COPY_DST);
        let stats = gpu.storage("Gamut Statistics", STAT_WORDS * 4, wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC);
        let scale = gpu.storage("Colour Space Scale", 8, wgpu::BufferUsages::empty());
        let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Gamut Statistics Staging"),
            size: STAT_WORDS * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_layout = stage::layout(
            gpu,
            "Colour Space",
            &[
                bind::texture(0, c, true),
                bind::storage(1, c, true),
                bind::storage(2, c, false),
                bind::storage(3, c, false),
                bind::storage(4, c, false),
                bind::storage(5, c, true),
                bind::uniform(6, c),
            ],
        );
        let compute = stage::compute_pipeline(gpu, "Colour Space Compute", COLOR_SPACE_WGSL, &compute_layout);
        let scale_layout = stage::layout(gpu, "Colour Space Scale", &[bind::storage(0, c, true), bind::storage(1, c, true), bind::storage(2, c, false)]);
        let scale_pipeline = stage::compute_pipeline(gpu, "Colour Space Scale", COLOR_SPACE_SCALE_WGSL, &scale_layout);
        let scale_group = stage::bind_group(
            gpu,
            "Colour Space Scale",
            &scale_layout,
            &[xy_bins.as_entire_binding(), vol_bins.as_entire_binding(), scale.as_entire_binding()],
        );

        let draw = |label: &str, code: &str, last_uniform: bool| {
            let layout = stage::layout(
                gpu,
                label,
                &[bind::storage(0, f, true), bind::storage(1, f, true), if last_uniform { bind::uniform(2, f) } else { bind::storage(2, f, true) }],
            );
            let module = stage::shader(gpu, label, code);
            (stage::render_pipeline(gpu, label, &module, &layout, "fs_main", CANVAS_FORMAT, None, &[]), layout)
        };
        let (chromaticity, chroma_layout) = draw("Chromaticity Draw", CHROMATICITY_DRAW_WGSL, false);
        let (volume, volume_layout) = draw("Colour Volume Draw", COLOR_VOLUME_DRAW_WGSL, true);
        let chromaticity_group = stage::bind_group(
            gpu,
            "Chromaticity Draw",
            &chroma_layout,
            &[xy_bins.as_entire_binding(), scale.as_entire_binding(), overlay.as_entire_binding()],
        );
        let volume_group = stage::bind_group(
            gpu,
            "Colour Volume Draw",
            &volume_layout,
            &[vol_bins.as_entire_binding(), scale.as_entire_binding(), camera.as_entire_binding()],
        );

        let cs = Self {
            calc,
            overlay,
            output_space,
            camera,
            xy_bins,
            vol_bins,
            stats,
            staging,
            compute_layout,
            compute,
            scale: scale_pipeline,
            scale_group,
            chromaticity,
            chromaticity_group,
            volume,
            volume_group,
            group: None,
            source: None,
            reading: Arc::new(Mutex::new(false)),
            timer: FrameTimer::new(gpu),
        };
        cs.set_output_space(gpu, gpu.display_space, None);
        cs.set_volume_view(gpu, 0.6, 0.42, 1.0);
        cs
    }

    pub fn set_source(&mut self, gpu: &Gpu, image: SourceImage, src_to_working: &Matrix3) {
        self.calc.update_source_space(src_to_working);
        let view = image.texture.create_view(&Default::default());
        self.group = Some(stage::bind_group(
            gpu,
            "Colour Space",
            &self.compute_layout,
            &[
                wgpu::BindingResource::TextureView(&view),
                self.calc.params.as_entire_binding(),
                self.xy_bins.as_entire_binding(),
                self.vol_bins.as_entire_binding(),
                self.stats.as_entire_binding(),
                self.overlay.as_entire_binding(),
                self.output_space.as_entire_binding(),
            ],
        ));
        self.source = Some(image);
    }

    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    /// Which triangles are drawn and counted, and what is measured.
    pub fn set_overlays(&self, gpu: &Gpu, enabled: [bool; 4], transformed: bool, volume: bool, destination: OutputSpace) {
        let mut words = vec![0u32; VOLUME_WORD + 1];
        let m = matrix_bytes(&space_to_xyz(destination));
        for (i, chunk) in m.chunks(4).enumerate() {
            words[i] = u32::from_le_bytes(chunk.try_into().unwrap());
        }
        for (slot, space) in OutputSpace::ALL.iter().enumerate() {
            let at = SPACES_OFFSET + slot * SPACE_WORDS;
            for (primary, xy) in space.primaries().iter().enumerate() {
                words[at + primary * 2] = (xy[0] as f32).to_bits();
                words[at + primary * 2 + 1] = (xy[1] as f32).to_bits();
            }
            words[at + 6] = enabled[slot] as u32;
            words[at + 7] = GAMUT_TINTS[slot];
        }
        for (i, [x, y]) in SPECTRAL_LOCUS.iter().enumerate() {
            words[LOCUS_OFFSET + i * 2] = x.to_bits();
            words[LOCUS_OFFSET + i * 2 + 1] = y.to_bits();
        }
        words[TRANSFORMED_WORD] = transformed as u32;
        words[VOLUME_WORD] = volume as u32;
        gpu.queue.write_buffer(&self.overlay, 0, bytemuck::cast_slice(&words));
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

    pub fn set_volume_view(&self, gpu: &Gpu, yaw: f32, pitch: f32, aspect: f32) {
        // Half the box's extent in clip units, enough to keep every corner in view.
        let floats = [yaw, pitch, aspect, 0.95];
        gpu.queue.write_buffer(&self.camera, 0, bytemuck::cast_slice(&floats));
    }

    fn draw_into(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture, pipeline: &wgpu::RenderPipeline, group: &wgpu::BindGroup) {
        let view = target.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Colour Space Draw"),
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
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..6, 0..1);
    }

    fn draws(&self, encoder: &mut wgpu::CommandEncoder, chromaticity: Option<&wgpu::Texture>, volume: Option<&wgpu::Texture>) {
        if let Some(t) = chromaticity {
            self.draw_into(encoder, t, &self.chromaticity, &self.chromaticity_group);
        }
        if let Some(t) = volume {
            self.draw_into(encoder, t, &self.volume, &self.volume_group);
        }
    }

    /// Bins the image and draws both plots. A target left out is not drawn.
    pub fn render(
        &mut self,
        gpu: &Gpu,
        sliders: &SlidersPayload,
        chromaticity: Option<&wgpu::Texture>,
        volume: Option<&wgpu::Texture>,
        measure: bool,
        done: impl FnOnce(Vec<PassTiming>, f64) + Send + 'static,
    ) {
        let (Some(group), Some(source)) = (&self.group, &self.source) else { return };
        let start = std::time::Instant::now();
        self.calc.update_sliders(sliders);
        if measure {
            self.timer.begin_frame();
        }
        let slots = if measure { [self.timer.claim("bin"), self.timer.claim("scale")] } else { [None; 2] };
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Colour Space") });
        self.calc.record(&mut encoder, None);
        encoder.clear_buffer(&self.xy_bins, 0, None);
        encoder.clear_buffer(&self.vol_bins, 0, None);
        encoder.clear_buffer(&self.stats, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Colour Space Pass"),
                timestamp_writes: self.timer.compute_writes(slots[0]),
            });
            pass.set_pipeline(&self.compute);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(source.width.div_ceil(16), source.height.div_ceil(16), 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Colour Space Scale Pass"),
                timestamp_writes: self.timer.compute_writes(slots[1]),
            });
            pass.set_pipeline(&self.scale);
            pass.set_bind_group(0, &self.scale_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.draws(&mut encoder, chromaticity, volume);
        if measure {
            self.timer.resolve(&mut encoder);
        }
        gpu.queue.submit(Some(encoder.finish()));
        let record_ms = start.elapsed().as_secs_f64() * 1000.0;
        if measure {
            self.timer.read(move |passes| done(passes, record_ms));
        }
    }

    /// Draws the plots again from the existing bins.
    pub fn redraw(&self, gpu: &Gpu, chromaticity: Option<&wgpu::Texture>, volume: Option<&wgpu::Texture>) {
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Colour Space Redraw") });
        self.draws(&mut encoder, chromaticity, volume);
        gpu.queue.submit(Some(encoder.finish()));
    }

    /// Reads the out-of-gamut tallies back. Only one read is in flight at a time.
    pub fn read_stats(&self, gpu: &Gpu, done: impl FnOnce(GamutStats) + Send + 'static) {
        if *self.reading.lock().unwrap() {
            return;
        }
        *self.reading.lock().unwrap() = true;
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Gamut Statistics Readback") });
        encoder.copy_buffer_to_buffer(&self.stats, 0, &self.staging, 0, STAT_WORDS * 4);
        gpu.queue.submit(Some(encoder.finish()));
        let staging = self.staging.clone();
        let reading = self.reading.clone();
        self.staging.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                let words: Vec<u32> = bytemuck::cast_slice(&staging.slice(..).get_mapped_range()).to_vec();
                staging.unmap();
                done(GamutStats { counted: words[0], outside: [words[1], words[2], words[3], words[4]] });
            }
            *reading.lock().unwrap() = false;
        });
    }
}
