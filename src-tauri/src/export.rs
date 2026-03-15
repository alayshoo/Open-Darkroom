// src-tauri/src/export.rs
//
// GPU-accelerated image export using the same WGSL shaders as the frontend.
// Runs calcParams.wgsl (compute) then develop.wgsl (render) via wgpu,
// processing the full-resolution image chunk by chunk.

use half::f16;
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
use tauri_plugin_dialog::DialogExt;
use wgpu::util::DeviceExt;

use crate::image_opening::{build_srgb_to_linear_lut_u16, ImageState};

// Embed the same shaders the frontend uses
const CALC_PARAMS_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/calcParams.wgsl");
const DEVELOP_WGSL: &str =
    include_str!("../../src/lib/gpu/shaders/develop.wgsl");

const SLIDERS_BYTES: u64 = 96;  // 24 × f32
const PARAMS_BYTES: u64  = 240; // Params struct
const CHUNK_ROWS: u32    = 512;

// ── Sliders payload ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
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
}

/// Pack sliders into a 96-byte uniform buffer matching the WGSL Sliders struct.
fn sliders_to_bytes(s: &SlidersPayload) -> [u8; 96] {
    let values: [f32; 24] = [
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
    ];
    let mut bytes = [0u8; 96];
    for (i, v) in values.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Convert a u16 RGBA chunk to f16 linear RGBA bytes for upload as rgba16float texture.
fn linearize_chunk(chunk_u16: &[u16], lut: &[f16]) -> Vec<u8> {
    let pixel_count = chunk_u16.len() / 4;
    let mut out = vec![0u8; pixel_count * 8]; // 4 × f16 = 8 bytes/pixel
    out.par_chunks_exact_mut(8)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            let r = lut[chunk_u16[src] as usize];
            let g = lut[chunk_u16[src + 1] as usize];
            let b = lut[chunk_u16[src + 2] as usize];
            let a = f16::from_f32(chunk_u16[src + 3] as f32 / 65535.0);
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });
    out
}

// ── Core GPU export ───────────────────────────────────────────────────────────

async fn run_export(
    pixels_u16: Vec<u16>,
    width: u32,
    height: u32,
    sliders: SlidersPayload,
    path: PathBuf,
) -> Result<(), String> {
    let t = Instant::now();

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
        contents: &sliders_to_bytes(&sliders),
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
            // @binding(0) inputTexture
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
            // @binding(1) texSampler
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            // @binding(2) params (storage read)
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

    let output_format = wgpu::TextureFormat::Rgba8Unorm;

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
                    format: output_format,
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

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // ── 5. Process image chunk by chunk ───────────────────────────────────────

    // Texture copy rows must be aligned to 256 bytes (COPY_BYTES_PER_ROW_ALIGNMENT)
    let bytes_per_row_unpadded = width * 4; // Rgba8Unorm = 4 bytes/pixel
    let padded_bytes_per_row =
        (bytes_per_row_unpadded + 255) & !255;

    let lut = build_srgb_to_linear_lut_u16();
    let total_pixels = (width * height) as usize;
    let mut output_rgb = vec![0u8; total_pixels * 3];

    let mut row_start = 0u32;
    let mut out_rgb_offset = 0usize;

    while row_start < height {
        let row_end = (row_start + CHUNK_ROWS).min(height);
        let chunk_height = row_end - row_start;
        let pixel_start = (row_start * width) as usize;
        let pixel_count = (chunk_height * width) as usize;

        // Linearise this chunk: u16 sRGB → f16 linear (rgba16float layout)
        let src_u16 = &pixels_u16[pixel_start * 4..(pixel_start + pixel_count) * 4];
        let chunk_f16_bytes = linearize_chunk(src_u16, &lut);

        // Input texture (rgba16float) for this chunk
        let input_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("input_tex"),
            size: wgpu::Extent3d {
                width,
                height: chunk_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &input_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &chunk_f16_bytes,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 8), // 8 bytes per rgba16float pixel
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width,
                height: chunk_height,
                depth_or_array_layers: 1,
            },
        );

        let input_view = input_tex.create_view(&Default::default());

        // Output texture (Rgba8Unorm) as render target for this chunk
        let output_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output_tex"),
            size: wgpu::Extent3d {
                width,
                height: chunk_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let output_view = output_tex.create_view(&Default::default());

        // Staging buffer for CPU readback
        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_buf"),
            size: (padded_bytes_per_row * chunk_height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Bind group for this chunk's input texture
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

        // Render + copy to staging
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("develop_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
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
            pass.set_pipeline(&dev_pipeline);
            pass.set_bind_group(0, &dev_bg, &[]);
            pass.draw(0..6, 0..1); // full-screen quad (6 vertices, hardcoded in vs_main)
        }
        enc.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &output_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &staging_buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height: chunk_height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(enc.finish()));

        // Map staging buffer and copy RGB into output (dropping alpha)
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
                let dst_base = out_rgb_offset + row * width as usize * 3;
                for px in 0..width as usize {
                    output_rgb[dst_base + px * 3]     = row_src[px * 4];
                    output_rgb[dst_base + px * 3 + 1] = row_src[px * 4 + 1];
                    output_rgb[dst_base + px * 3 + 2] = row_src[px * 4 + 2];
                }
            }
        }
        staging_buf.unmap();

        out_rgb_offset += chunk_height as usize * width as usize * 3;
        row_start = row_end;

        println!(
            "export: processed rows {}-{} / {}",
            row_start - chunk_height,
            row_end,
            height
        );
    }

    // ── 6. Save as PNG ────────────────────────────────────────────────────────

    let img = image::RgbImage::from_raw(width, height, output_rgb)
        .ok_or("Failed to assemble output image")?;
    img.save(&path)
        .map_err(|e| format!("Failed to save PNG: {e}"))?;

    println!(
        "export: saved {} × {} → {} in {} ms",
        width,
        height,
        path.display(),
        t.elapsed().as_millis()
    );

    Ok(())
}

// ── Tauri command ─────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn export_image(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    sliders: SlidersPayload,
) -> Result<(), String> {
    println!("export_image: opening save dialog");

    let path = app
        .dialog()
        .file()
        .add_filter("PNG Image", &["png"])
        .blocking_save_file()
        .ok_or("No file selected")?
        .into_path()
        .map_err(|e| e.to_string())?;

    let path = if path.extension().map(|e| e.to_ascii_lowercase()) != Some("png".into()) {
        path.with_extension("png")
    } else {
        path
    };

    let (pixels_u16, width, height) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        (img.pixels_u16.clone(), img.width, img.height)
    };

    run_export(pixels_u16, width, height, sliders, path).await
}
