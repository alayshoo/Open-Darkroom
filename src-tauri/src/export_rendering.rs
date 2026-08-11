// src-tauri/src/export_rendering.rs
//
// GPU-accelerated image rendering pipeline using the same WGSL shaders as the
// frontend. Runs calcParams.wgsl (compute) then develop.wgsl (render) via wgpu,
// processing the full-resolution image chunk by chunk.
// Returns raw RGB bytes ready for encoding.
//
// This module is deliberately free of Tauri: it takes plain pixels and a
// progress callback, so it can be driven headlessly from tests as well as from
// the export command.

use std::time::Instant;
use wgpu::util::DeviceExt;

use crate::color::{build_srgb_to_linear_lut_u16, linearize_rgba_u16, LINEAR_BYTES_PER_PIXEL};

pub const CALC_PARAMS_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/calcParams.wgsl");
pub const DEVELOP_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/develop.wgsl");
pub const COMPOSITE_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/composite.wgsl");
pub const COLOR_SPACE_ENCODE_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/colorSpaceEncode.wgsl");

/// Format the render chain hands between its stages. Must match
/// `WORKING_FORMAT` in `src/lib/types/gpuTypes.ts` — see the note there on why
/// the intermediates are 32-bit float rather than 16.
pub const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// Size of the `Params` struct written by calcParams.wgsl and read by
/// develop.wgsl / histogram.wgsl.
pub const PARAMS_BYTES: u64 = 256;

/// Size of the `Sliders` uniform consumed by calcParams.wgsl. The struct's 30
/// f32 fields occupy 120 bytes; a uniform struct is bound 16-aligned, so the
/// last 8 bytes are tail padding and are never written.
pub const SLIDERS_BYTES: usize = 128;

/// Number of f32 fields in the `Sliders` struct, ahead of the tail padding.
pub const SLIDER_COUNT: usize = 30;

/// Size of the `View` uniform consumed by calcParams.wgsl. One f32 of payload,
/// padded to the 16 bytes a uniform struct binds at.
pub const VIEW_BYTES: usize = 16;

/// Rows rendered per GPU pass. Bounds peak VRAM for very large images.
pub const CHUNK_ROWS: u32 = 512;

// ── Sliders payload ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlidersPayload {
    pub invert:            bool,
    pub red_black_point:   f32,
    pub green_black_point: f32,
    pub blue_black_point:  f32,
    pub red_white_point:   f32,
    pub green_white_point: f32,
    pub blue_white_point:  f32,
    pub rgb_output_black:  f32,
    pub rgb_output_white:  f32,
    pub red_gamma:         f32,
    pub green_gamma:       f32,
    pub blue_gamma:        f32,
    pub wb_temp:           f32,
    pub wb_tint:           f32,
    pub exposure:          f32,
    pub contrast:          f32,
    pub brightness:        f32,
    pub highlights:        f32,
    pub shadows:           f32,
    pub whites:            f32,
    pub blacks:            f32,
    pub saturation:        f32,
    pub vibrance:          f32,
    pub hue:               f32,
    pub clarity:              f32,
    pub texture:              f32,
    pub usm_amount:           f32,
    pub usm_radius:           f32,
    pub usm_luma_threshold:   f32,
    pub usm_detail_threshold: f32,
}

impl Default for SlidersPayload {
    /// Neutral sliders — must stay in step with `defaultSlidersRGB` in
    /// `src/lib/types/imgParameters.ts`. These values make the develop chain a
    /// mathematical no-op, which is what the identity test relies on.
    fn default() -> Self {
        Self {
            invert:            false,
            red_black_point:   0.0,
            green_black_point: 0.0,
            blue_black_point:  0.0,
            red_white_point:   255.0,
            green_white_point: 255.0,
            blue_white_point:  255.0,
            rgb_output_black:  0.0,
            rgb_output_white:  255.0,
            red_gamma:         1.0,
            green_gamma:       1.0,
            blue_gamma:        1.0,
            wb_temp:           5500.0,
            wb_tint:           0.0,
            exposure:          0.0,
            contrast:          0.0,
            brightness:        0.0,
            highlights:        0.0,
            shadows:           0.0,
            whites:            0.0,
            blacks:            0.0,
            saturation:        0.0,
            vibrance:          0.0,
            hue:               0.0,
            clarity:              0.0,
            texture:              0.0,
            usm_amount:           0.0,
            usm_radius:           1.0,
            usm_luma_threshold:   0.0,
            usm_detail_threshold: 0.0,
        }
    }
}

/// Pack sliders into the uniform buffer matching the WGSL Sliders struct.
///
/// The order here must match `Sliders` in calcParams.wgsl *and* `slidersToArray`
/// in calcParamsPipeline.ts — all three are the same struct viewed from three
/// languages, and a reordering in one is silent corruption in the others.
pub fn sliders_to_bytes(s: &SlidersPayload) -> [u8; SLIDERS_BYTES] {
    let values: [f32; SLIDER_COUNT] = [
        if s.invert { 1.0 } else { 0.0 },
        s.red_black_point,
        s.green_black_point,
        s.blue_black_point,
        s.red_white_point,
        s.green_white_point,
        s.blue_white_point,
        s.rgb_output_black,
        s.rgb_output_white,
        s.red_gamma,
        s.green_gamma,
        s.blue_gamma,
        s.wb_temp,
        s.wb_tint,
        s.exposure,
        s.contrast,
        s.brightness,
        s.highlights,
        s.shadows,
        s.whites,
        s.blacks,
        s.saturation,
        s.vibrance,
        s.hue,
        s.clarity,
        s.texture,
        s.usm_amount,
        s.usm_radius,
        s.usm_luma_threshold,
        s.usm_detail_threshold,
    ];
    let mut bytes = [0u8; SLIDERS_BYTES];
    for (i, v) in values.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

// ── View payload ──────────────────────────────────────────────────────────────

/// Where a render sits relative to the full-resolution image.
///
/// Sliders measured in image pixels — the unsharp mask radius — mean
/// full-resolution pixels, so a render working on a downscaled surface has to
/// scale them. This is separate from `SlidersPayload` because it tracks the view
/// rather than an edit: it changes on zoom, not when a slider moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewPayload {
    /// Rendered pixels per full-resolution image pixel. Below 1 for a
    /// downscaled preview, exactly 1 for an export.
    pub render_scale: f32,
}

impl Default for ViewPayload {
    /// Full resolution — the export path never renders anywhere else.
    fn default() -> Self {
        Self { render_scale: 1.0 }
    }
}

/// Pack the view into the uniform buffer matching the WGSL `View` struct.
pub fn view_to_bytes(v: &ViewPayload) -> [u8; VIEW_BYTES] {
    let mut bytes = [0u8; VIEW_BYTES];
    bytes[0..4].copy_from_slice(&v.render_scale.to_le_bytes());
    bytes
}

// ── Core GPU render ───────────────────────────────────────────────────────────

/// Run the GPU pipeline on the full image and return raw RGB bytes (no alpha).
///
/// See [`render_image_with_chunk_rows`]; this uses the default [`CHUNK_ROWS`].
pub async fn render_image(
    pixels_u16: &[u16],
    width: u32,
    height: u32,
    sliders: &SlidersPayload,
    bit_depth: u8,
    on_progress: impl Fn(f32) + Send + Sync,
) -> Result<Vec<u8>, String> {
    render_image_with_chunk_rows(
        pixels_u16,
        width,
        height,
        sliders,
        bit_depth,
        CHUNK_ROWS,
        on_progress,
    )
    .await
}

/// Run the GPU pipeline on the full image and return raw RGB bytes (no alpha).
///
/// When `bit_depth` is 8 the output texture is `Rgba8Unorm` and the returned
/// buffer contains 3 × u8 per pixel.  When `bit_depth` is 16 the output
/// texture is `Rgba16Float`; each channel is read back as an f16, converted to
/// a u16 (0–65535), and stored little-endian, giving 6 bytes per pixel.
///
/// `pixels_u16` is interleaved RGBA at `width` × `height`. The image is
/// rendered `chunk_rows` rows at a time; `on_progress` is called after each
/// chunk with the fraction completed, in (0, 1].
///
/// `chunk_rows` is exposed so tests can vary the chunking — rendering an image
/// in one pass and in several must produce identical output.
pub async fn render_image_with_chunk_rows(
    pixels_u16: &[u16],
    width: u32,
    height: u32,
    sliders: &SlidersPayload,
    bit_depth: u8,
    chunk_rows: u32,
    on_progress: impl Fn(f32) + Send + Sync,
) -> Result<Vec<u8>, String> {
    let t = Instant::now();

    // ── 0. Validate inputs ────────────────────────────────────────────────────

    if width == 0 || height == 0 {
        return Err(format!("Cannot render a {width} × {height} image"));
    }
    if chunk_rows == 0 {
        return Err("chunk_rows must be non-zero".to_string());
    }
    let expected = (width as usize) * (height as usize) * 4;
    if pixels_u16.len() != expected {
        return Err(format!(
            "Pixel buffer is {} u16 values, expected {expected} for {width} × {height} RGBA",
            pixels_u16.len()
        ));
    }

    // ── 1. Initialise wgpu ────────────────────────────────────────────────────

    let instance = wgpu::Instance::default();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("No GPU adapter found: {e}"))?;

    let info = adapter.get_info();
    println!("export: adapter = {} ({:?})", info.name, info.backend);

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("export_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .map_err(|e| format!("GPU device creation failed: {e}"))?;

    // ── 2. Slider uniform + params storage buffers ────────────────────────────

    let slider_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("slider_buf"),
        contents: &sliders_to_bytes(sliders),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("params_buf"),
        size: PARAMS_BYTES,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    // ── 3. calcParams compute pipeline ────────────────────────────────────────

    let calc_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("calcParams"),
        source: wgpu::ShaderSource::Wgsl(CALC_PARAMS_WGSL.into()),
    });

    let calc_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("calc_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let calc_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&calc_bgl],
            push_constant_ranges: &[],
        });

    let calc_pipeline =
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("calc_pipeline"),
            layout: Some(&calc_pipeline_layout),
            module: &calc_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

    // Export renders at full resolution, so the view is the default 1.0.
    let view_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("view_buf"),
        contents: &view_to_bytes(&ViewPayload::default()),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let calc_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("calc_bg"),
        layout: &calc_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: slider_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: view_buf.as_entire_binding(),
            },
        ],
    });

    // Run calcParams once — computes all matrices from slider values
    {
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&calc_pipeline);
            pass.set_bind_group(0, &calc_bg, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        queue.submit(std::iter::once(enc.finish()));
        let _ = device.poll(wgpu::MaintainBase::Wait);
    }

    // ── 4. develop render pipeline ────────────────────────────────────────────

    let develop_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("develop"),
        source: wgpu::ShaderSource::Wgsl(DEVELOP_WGSL.into()),
    });

    let dev_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dev_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let dev_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dev_layout"),
            bind_group_layouts: &[&dev_bgl],
            push_constant_ranges: &[],
        });

    let output_format = if bit_depth >= 16 {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };

    // Develop writes the perceptual working signal, not the display surface.
    let dev_pipeline =
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dev_pipeline"),
            layout: Some(&dev_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &develop_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &develop_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: WORKING_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

    // ── 4b. composite and output-transform pipelines ──────────────────────────
    //
    // Both read one texture and write one full-screen target, so they share a
    // layout. 32-bit float textures are not filterable and these stages index
    // their input one to one with textureLoad, so no sampler is bound.

    let fs_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fs_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });

    let fs_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fs_layout"),
            bind_group_layouts: &[&fs_bgl],
            push_constant_ranges: &[],
        });

    let mut full_screen_pipeline = |label: &str, source: &str, format| {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&fs_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    };

    let composite_pipeline =
        full_screen_pipeline("composite_pipeline", COMPOSITE_WGSL, WORKING_FORMAT);
    let encode_pipeline = full_screen_pipeline(
        "color_space_encode_pipeline",
        COLOR_SPACE_ENCODE_WGSL,
        output_format,
    );

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // ── 5. Process image chunk by chunk ───────────────────────────────────────

    // Rgba8Unorm = 4 bytes/pixel, Rgba16Float = 8 bytes/pixel
    let bytes_per_texel = if bit_depth >= 16 { 8u32 } else { 4u32 };
    let bytes_per_row_unpadded = width * bytes_per_texel;
    let padded_bytes_per_row = (bytes_per_row_unpadded + 255) & !255;

    // Output buffer: 3 × u8 per pixel for 8-bit, 3 × u16-as-LE (6 bytes) for 16-bit
    let bytes_per_output_pixel = if bit_depth >= 16 { 6usize } else { 3usize };
    let lut = build_srgb_to_linear_lut_u16();
    let total_pixels = (width * height) as usize;
    let mut output_rgb = vec![0u8; total_pixels * bytes_per_output_pixel];

    let mut row_start = 0u32;
    let mut out_rgb_offset = 0usize;

    // Intermediates carrying the perceptual working signal between stages. Every
    // chunk is the same height but the last, so these are built once and reused
    // rather than reallocated per chunk — at 16 bytes per texel they are the
    // largest allocations in the loop.
    let mut intermediates: Option<(u32, wgpu::TextureView, wgpu::TextureView)> = None;

    while row_start < height {
        let row_end = (row_start + chunk_rows).min(height);
        let chunk_height = row_end - row_start;
        let pixel_start = (row_start * width) as usize;
        let pixel_count = (chunk_height * width) as usize;

        let src_u16 = &pixels_u16[pixel_start * 4..(pixel_start + pixel_count) * 4];
        let chunk_f16_bytes = linearize_rgba_u16(src_u16, &lut);

        let input_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("input_tex"),
            size: wgpu::Extent3d { width, height: chunk_height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &input_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &chunk_f16_bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * LINEAR_BYTES_PER_PIXEL as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d { width, height: chunk_height, depth_or_array_layers: 1 },
        );

        let input_view = input_tex.create_view(&Default::default());

        if intermediates.as_ref().map(|(h, _, _)| *h) != Some(chunk_height) {
            let intermediate = |label| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width,
                            height: chunk_height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: WORKING_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            intermediates = Some((
                chunk_height,
                intermediate("develop_tex"),
                intermediate("composite_tex"),
            ));
        }
        let (_, develop_view, composite_view) = intermediates.as_ref().unwrap();

        let output_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_tex"),
            size: wgpu::Extent3d { width, height: chunk_height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let output_view = output_tex.create_view(&Default::default());

        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_buf"),
            size: (padded_bytes_per_row * chunk_height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let dev_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dev_bg"),
            layout: &dev_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        let full_screen_bg = |label, input: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &fs_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input),
                }],
            })
        };
        let composite_bg = full_screen_bg("composite_bg", develop_view);
        let encode_bg = full_screen_bg("encode_bg", composite_view);

        let mut enc = device.create_command_encoder(&Default::default());

        // Each stage reads the previous one's target, so they cannot be merged:
        // a fragment can only read a texture it is not currently writing.
        for (label, target, pipeline, bind_group) in [
            ("develop_pass", develop_view, &dev_pipeline, &dev_bg),
            ("composite_pass", composite_view, &composite_pipeline, &composite_bg),
            ("encode_pass", &output_view, &encode_pipeline, &encode_bg),
        ] {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..6, 0..1);
        }

        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d { width, height: chunk_height, depth_or_array_layers: 1 },
        );
        queue.submit(std::iter::once(enc.finish()));

        let (tx, rx) = std::sync::mpsc::channel();
        staging_buf
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        let _ = device.poll(wgpu::MaintainBase::Wait);
        rx.recv()
            .unwrap()
            .map_err(|e| format!("GPU buffer map failed: {e}"))?;

        {
            let mapped = staging_buf.slice(..).get_mapped_range();
            for row in 0..chunk_height as usize {
                let row_src = &mapped[row * padded_bytes_per_row as usize
                    ..row * padded_bytes_per_row as usize + bytes_per_row_unpadded as usize];
                let dst_base = out_rgb_offset + row * width as usize * bytes_per_output_pixel;
                if bit_depth >= 16 {
                    // Each channel is an f16 (2 bytes LE); convert to u16 and store LE.
                    for px in 0..width as usize {
                        let s = px * 8; // 8 bytes per Rgba16Float texel
                        let r = half::f16::from_le_bytes([row_src[s],     row_src[s + 1]]);
                        let g = half::f16::from_le_bytes([row_src[s + 2], row_src[s + 3]]);
                        let b = half::f16::from_le_bytes([row_src[s + 4], row_src[s + 5]]);
                        let r16 = (r.to_f32().clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                        let g16 = (g.to_f32().clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                        let b16 = (b.to_f32().clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                        let d = dst_base + px * 6;
                        output_rgb[d..d + 2].copy_from_slice(&r16.to_le_bytes());
                        output_rgb[d + 2..d + 4].copy_from_slice(&g16.to_le_bytes());
                        output_rgb[d + 4..d + 6].copy_from_slice(&b16.to_le_bytes());
                    }
                } else {
                    for px in 0..width as usize {
                        output_rgb[dst_base + px * 3]     = row_src[px * 4];
                        output_rgb[dst_base + px * 3 + 1] = row_src[px * 4 + 1];
                        output_rgb[dst_base + px * 3 + 2] = row_src[px * 4 + 2];
                    }
                }
            }
        }
        staging_buf.unmap();

        out_rgb_offset += chunk_height as usize * width as usize * bytes_per_output_pixel;
        row_start = row_end;

        let progress = row_end as f32 / height as f32;
        on_progress(progress);

        println!(
            "export: rendered rows {}-{} / {} ({:.0}%) in {} ms total",
            row_start - chunk_height,
            row_end,
            height,
            progress * 100.0,
            t.elapsed().as_millis(),
        );
    }

    Ok(output_rgb)
}
