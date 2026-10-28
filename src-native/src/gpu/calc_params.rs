// src-native/src/gpu/calc_params.rs
//
// The calcParams compute pass and the buffers around it. Mirrors
// calcParamsPipeline.ts; the packing is export_rendering.rs's, so the preview
// and the export write the uniforms byte for byte alike.

use wgpu::ShaderStages as S;

use super::{bind, stage, Gpu};
use crate::export_rendering::{
    curves_to_bytes, sliders_to_bytes, tone_map_to_bytes, view_to_bytes, SlidersPayload, ToneMapPayload, ViewPayload,
    CURVES_OFFSET, PARAMS_BYTES, SHARP_PARAMS_BYTES, SLIDERS_BYTES, SOURCE_SPACE_BYTES, TONE_MAP_BYTES, VIEW_BYTES,
};
use crate::icc::{Matrix3, IDENTITY};
use crate::shaders::CALC_PARAMS_WGSL;

pub struct CalcParams {
    sliders: wgpu::Buffer,
    pub params: wgpu::Buffer,
    view: wgpu::Buffer,
    pub sharp: wgpu::Buffer,
    source: wgpu::Buffer,
    tone_map: wgpu::Buffer,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    queue: wgpu::Queue,
}

/// A WGSL mat3x3<f32> is three 16-byte columns, not nine packed floats.
pub fn matrix_bytes(m: &Matrix3) -> [u8; 48] {
    let mut bytes = [0u8; 48];
    for col in 0..3 {
        for row in 0..3 {
            let at = col * 16 + row * 4;
            bytes[at..at + 4].copy_from_slice(&m[col * 3 + row].to_le_bytes());
        }
    }
    bytes
}

impl CalcParams {
    pub fn new(gpu: &Gpu) -> Self {
        let sliders = gpu.uniform("Sliders Uniform Buffer", SLIDERS_BYTES as u64);
        let params = gpu.storage("Computed Params Storage Buffer", PARAMS_BYTES, wgpu::BufferUsages::COPY_DST);
        let view = gpu.uniform("View Uniform Buffer", VIEW_BYTES as u64);
        let sharp = gpu.storage("Computed Sharp Params Storage Buffer", SHARP_PARAMS_BYTES, wgpu::BufferUsages::empty());
        let source = gpu.uniform("Source Space Uniform Buffer", SOURCE_SPACE_BYTES);
        let tone_map = gpu.uniform("Tone Map Uniform Buffer", TONE_MAP_BYTES as u64);

        let layout = stage::layout(
            gpu,
            "Calc Params Bind Group Layout",
            &[
                bind::uniform(0, S::COMPUTE),
                bind::storage(1, S::COMPUTE, false),
                bind::uniform(2, S::COMPUTE),
                bind::storage(3, S::COMPUTE, false),
                bind::uniform(4, S::COMPUTE),
                bind::uniform(5, S::COMPUTE),
            ],
        );
        let pipeline = stage::compute_pipeline(gpu, "Calc Params Compute Pipeline", CALC_PARAMS_WGSL, &layout);
        let bind_group = stage::bind_group(
            gpu,
            "Calc Params Bind Group",
            &layout,
            &[
                sliders.as_entire_binding(),
                params.as_entire_binding(),
                view.as_entire_binding(),
                sharp.as_entire_binding(),
                source.as_entire_binding(),
                tone_map.as_entire_binding(),
            ],
        );

        let c = Self { sliders, params, view, sharp, source, tone_map, pipeline, bind_group, queue: gpu.queue.clone() };
        // A render is full resolution until something says otherwise.
        c.update_view(1.0, 0.0, 0.0);
        c.update_source_space(&IDENTITY);
        c.update_tone_map(&ToneMapPayload::default());
        c.update_curves(None);
        c.update_sliders(&SlidersPayload::default());
        c
    }

    pub fn update_sliders(&self, s: &SlidersPayload) {
        self.queue.write_buffer(&self.sliders, 0, &sliders_to_bytes(s));
    }

    pub fn update_view(&self, render_scale: f32, full_width: f32, full_height: f32) {
        let bytes = view_to_bytes(&ViewPayload { render_scale, full_width, full_height });
        self.queue.write_buffer(&self.view, 0, &bytes);
    }

    pub fn update_source_space(&self, to_working: &Matrix3) {
        self.queue.write_buffer(&self.source, 0, &matrix_bytes(to_working));
    }

    pub fn update_tone_map(&self, t: &ToneMapPayload) {
        self.queue.write_buffer(&self.tone_map, 0, &tone_map_to_bytes(t));
    }

    /// A None table only clears the flag; the stale entries behind it are never read.
    pub fn update_curves(&self, lut: Option<&[f32]>) {
        let bytes = curves_to_bytes(lut.unwrap_or(&[])).unwrap_or_else(|_| vec![0u8; 16]);
        self.queue.write_buffer(&self.params, CURVES_OFFSET, &bytes);
    }

    pub fn record(&self, encoder: &mut wgpu::CommandEncoder, timestamps: Option<wgpu::ComputePassTimestampWrites<'_>>) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Calc Params Pass"),
            timestamp_writes: timestamps,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
}
