// src-native/src/gpu/source.rs
//
// The image on the GPU: the preview straight after an open, then the full
// resolution streamed in behind it a band of rows at a time, into one texture
// whose mip levels are built on the GPU. Mirrors fullResolution.ts.

use wgpu::ShaderStages as S;

use super::{bind, stage, Gpu, SOURCE_FORMAT};
use crate::color::LINEAR_BYTES_PER_PIXEL;
use crate::shaders::MIPMAP_WGSL;

/// Roughly how much one band carries, so no single transfer grows unbounded.
const BAND_BYTES: usize = 32 * 1024 * 1024;

/// A source the chain samples, with every mip level filled.
pub struct SourceImage {
    pub texture: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

/// Rows per band for an image `width` pixels wide.
pub fn band_rows(width: u32) -> u32 {
    ((BAND_BYTES / (width as usize * LINEAR_BYTES_PER_PIXEL)) as u32).max(1)
}

pub fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

pub struct Mipmapper {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Mipmapper {
    pub fn new(gpu: &Gpu) -> Self {
        let layout = stage::layout(
            gpu,
            "Mipmap Bind Group Layout",
            &[bind::texture(0, S::FRAGMENT, true), bind::sampler(1, S::FRAGMENT, true)],
        );
        let module = stage::shader(gpu, "Mipmap Shader", MIPMAP_WGSL);
        let pipeline = stage::render_pipeline(gpu, "Mipmap Pipeline", &module, &layout, "fs_main", SOURCE_FORMAT, None, &[]);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mipmap Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { pipeline, layout, sampler }
    }

    /// Fills every level of `texture` below the first from the one above it.
    pub fn generate(&self, gpu: &Gpu, texture: &wgpu::Texture) {
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Generate Mipmaps") });
        let level_view = |level: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        for level in 1..texture.mip_level_count() {
            let parent = level_view(level - 1);
            let bind_group = stage::bind_group(
                gpu,
                "Mipmap Bind Group",
                &self.layout,
                &[wgpu::BindingResource::TextureView(&parent), wgpu::BindingResource::Sampler(&self.sampler)],
            );
            let target = level_view(level);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Mipmap Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        gpu.queue.submit(Some(encoder.finish()));
    }
}

/// An empty mipmapped source of the given size.
pub fn source_texture(gpu: &Gpu, label: &str, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: mip_level_count(width, height),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SOURCE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// Write rows `y..y + rows` of RGBA f16 pixels into the top level.
pub fn write_rows(gpu: &Gpu, texture: &wgpu::Texture, width: u32, y: u32, rows: u32, pixels: &[u8]) {
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * LINEAR_BYTES_PER_PIXEL as u32),
            rows_per_image: Some(rows),
        },
        wgpu::Extent3d { width, height: rows, depth_or_array_layers: 1 },
    );
}

/// The whole of a small image, uploaded at once, mipmapped or not.
pub fn upload(gpu: &Gpu, mipmapper: Option<&Mipmapper>, label: &str, pixels: &[u8], width: u32, height: u32) -> SourceImage {
    let texture = match mipmapper {
        Some(_) => source_texture(gpu, label, width, height),
        None => gpu.texture(label, width, height, SOURCE_FORMAT, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST),
    };
    write_rows(gpu, &texture, width, 0, height, pixels);
    if let Some(m) = mipmapper {
        m.generate(gpu, &texture);
    }
    SourceImage { texture, width, height }
}
