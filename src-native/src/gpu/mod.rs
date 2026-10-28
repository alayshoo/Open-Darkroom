// src-native/src/gpu/mod.rs
//
// The GPU side of the preview: the same render chain the Tauri frontend runs
// through WebGPU, here through wgpu on the device egui draws with, so the
// image never leaves the GPU on its way to the window.

pub mod calc_params;
pub mod chrome;
pub mod color_space;
pub mod frame_timer;
pub mod histogram;
pub mod renderer;
pub mod source;
pub mod stage;

use std::sync::Arc;

use crate::output_space::OutputSpace;

/// Format the preview's chain hands between its stages. Half float to halve
/// the memory of window-sized textures; the export keeps 32-bit.
pub const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format the view sample stage resamples the source into. Filterable, which
/// develop needs since it samples rather than loads.
pub const VIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the sharpness textures, at the working format's precision.
pub const SHARP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the source image, as the backend linearises it.
pub const SOURCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the textures drawn as interface: the preview canvas and the
/// plots. Their values are already encoded for the display, which is what
/// egui expects of a texture, so they reach the window unchanged.
pub const CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The device and what was learned about it while asking for it.
#[derive(Clone)]
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub target_format: wgpu::TextureFormat,
    pub max_texture: u32,
    pub can_timestamp: bool,
    /// The space the preview is encoded for. A window surface is sRGB.
    pub display_space: OutputSpace,
    pub adapter_info: wgpu::AdapterInfo,
    pub adapter_features: wgpu::Features,
    pub renderer: Arc<egui::mutex::RwLock<egui_wgpu::Renderer>>,
}

impl Gpu {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let features = rs.device.features();
        Self {
            device: rs.device.clone(),
            queue: rs.queue.clone(),
            target_format: rs.target_format,
            max_texture: rs.device.limits().max_texture_dimension_2d,
            can_timestamp: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            display_space: OutputSpace::Srgb,
            adapter_info: rs.adapter.get_info(),
            adapter_features: rs.adapter.features(),
            renderer: rs.renderer.clone(),
        }
    }

    pub fn uniform(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn storage(&self, label: &str, size: u64, extra: wgpu::BufferUsages) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | extra,
            mapped_at_creation: false,
        })
    }

    pub fn texture(&self, label: &str, width: u32, height: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    /// A texture the GPU draws and egui shows, with the sRGB view egui reads.
    pub fn canvas_texture(&self, label: &str, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CANVAS_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Hand a canvas texture to egui, replacing whatever `id` showed before.
    pub fn register_canvas(&self, texture: &wgpu::Texture, id: Option<egui::TextureId>, filter: wgpu::FilterMode) -> egui::TextureId {
        let view = texture.create_view(&Default::default());
        let mut renderer = self.renderer.write();
        match id {
            Some(id) => {
                renderer.update_egui_texture_from_wgpu_texture(&self.device, &view, filter, id);
                id
            }
            None => renderer.register_native_texture(&self.device, &view, filter),
        }
    }
}

/// Bind group layout entries the stages share.
pub mod bind {
    pub fn uniform(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }
    }

    pub fn storage(binding: u32, vis: wgpu::ShaderStages, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }
    }

    pub fn texture(binding: u32, vis: wgpu::ShaderStages, filterable: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }
    }

    pub fn sampler(binding: u32, vis: wgpu::ShaderStages, filtering: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Sampler(if filtering {
                wgpu::SamplerBindingType::Filtering
            } else {
                wgpu::SamplerBindingType::NonFiltering
            }),
            count: None,
        }
    }

    pub fn entries<'a>(resources: &[wgpu::BindingResource<'a>]) -> Vec<wgpu::BindGroupEntry<'a>> {
        resources
            .iter()
            .enumerate()
            .map(|(i, r)| wgpu::BindGroupEntry { binding: i as u32, resource: r.clone() })
            .collect()
    }
}
