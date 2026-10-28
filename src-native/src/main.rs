// src-native/src/main.rs
//
// Open Darkroom as a single Rust process: egui for the interface, wgpu for the
// image, and the Tauri app's backend compiled in from its own sources.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../../src-tauri/src/color.rs"]
mod color;
#[path = "../../src-tauri/src/crop.rs"]
mod crop;
#[path = "../../src-tauri/src/export_encoding.rs"]
mod export_encoding;
#[path = "../../src-tauri/src/export_rendering.rs"]
mod export_rendering;
#[path = "../../src-tauri/src/full_resolution.rs"]
mod full_resolution;
#[path = "../../src-tauri/src/icc.rs"]
mod icc;
#[path = "../../src-tauri/src/image_opening.rs"]
mod image_opening;
#[path = "../../src-tauri/src/output_space.rs"]
mod output_space;
#[path = "../../src-tauri/src/raw.rs"]
mod raw;

mod app;
mod gpu;
mod model;
mod shaders;
mod ui;

use std::sync::Arc;

fn icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/app-icon.png")).expect("bundled icon").to_rgba8();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

fn main() -> eframe::Result {
    let setup = egui_wgpu::WgpuSetupCreateNew {
        // The largest texture the adapter allows rather than the default 8192,
        // so most full-resolution images fit in one, and timestamps where the
        // adapter offers them, for the frame stats.
        device_descriptor: Arc::new(|adapter| {
            let limits = adapter.limits();
            let timestamps = adapter.features() & wgpu::Features::TIMESTAMP_QUERY;
            wgpu::DeviceDescriptor {
                label: Some("Open Darkroom"),
                required_features: timestamps,
                required_limits: wgpu::Limits {
                    max_texture_dimension_2d: limits.max_texture_dimension_2d,
                    max_buffer_size: limits.max_buffer_size,
                    max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
                    ..wgpu::Limits::default()
                },
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            }
        }),
        ..Default::default()
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Open Darkroom")
            .with_inner_size([1000.0, 800.0])
            .with_min_inner_size([700.0, 500.0])
            .with_decorations(false)
            .with_icon(icon()),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: setup.into(),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native("Open Darkroom", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
