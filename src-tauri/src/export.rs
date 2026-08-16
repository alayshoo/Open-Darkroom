// src-tauri/src/export.rs
//
// Tauri command handler for image export.
// Orchestrates: file dialog → GPU render (export_rendering) → encode (export_encoding).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

use crate::export_encoding::{encode_and_save, ExportFormat, ExportSettings};
use crate::export_rendering::{render_image, SlidersPayload};
use crate::image_opening::ImageState;

// ── Tauri command ─────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn export_image(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    sliders: SlidersPayload,
    settings: ExportSettings,
) -> Result<(), String> {
    println!("export_image: opening save dialog ({:?})", settings.format);

    let (filter_name, extensions): (&str, &[&str]) = match settings.format {
        ExportFormat::Png => ("PNG Image", &["png"]),
        ExportFormat::Jpeg => ("JPEG Image", &["jpg", "jpeg"]),
        ExportFormat::Webp => ("WebP Image", &["webp"]),
        ExportFormat::Tiff => ("TIFF Image", &["tiff", "tif"]),
    };

    let path = app
        .dialog()
        .file()
        .add_filter(filter_name, extensions)
        .blocking_save_file()
        .ok_or("No file selected")?
        .into_path()
        .map_err(|e| e.to_string())?;

    let path = match settings.format {
        ExportFormat::Png => ensure_extension(path, "png"),
        ExportFormat::Jpeg => ensure_extension(path, "jpg"),
        ExportFormat::Webp => ensure_extension(path, "webp"),
        ExportFormat::Tiff => ensure_extension(path, "tiff"),
    };

    let (pixels_u16, width, height, channels) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        (
            Arc::clone(&img.pixels_u16),
            img.width,
            img.height,
            img.channels,
        )
    };

    let t = Instant::now();

    let bit_depth = match settings.format {
        ExportFormat::Tiff => settings.tiff_bit_depth,
        _ => 8,
    };

    let _ = app.emit("export:started", ());

    // The renderer is Tauri-agnostic; bridging its progress to the frontend is
    // this command's job.
    let progress_handle = app.clone();
    let rgb_bytes = render_image(
        &pixels_u16,
        width,
        height,
        channels,
        &sliders,
        bit_depth,
        move |progress| {
            let _ = progress_handle.emit("export:progress", progress);
        },
    )
    .await?;

    println!(
        "export: GPU render done in {} ms, encoding…",
        t.elapsed().as_millis()
    );

    encode_and_save(rgb_bytes, width, height, &settings, &path)?;

    let _ = app.emit("export:progress", 1.0f32);

    println!(
        "export: saved {} × {} → {} in {} ms total",
        width,
        height,
        path.display(),
        t.elapsed().as_millis(),
    );

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().map(|e| e.to_ascii_lowercase()) != Some(ext.into()) {
        path.with_extension(ext)
    } else {
        path
    }
}
