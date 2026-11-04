// src-tauri/src/export.rs
//
// Tauri command handler for image export.
// Orchestrates: file dialog → GPU render (export_rendering) → encode (export_encoding).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

use crate::export_encoding::{encode_and_save_with_progress, ExportFormat, ExportSettings};
use crate::export_progress::{ProgressEvent, ProgressPlan};
use crate::crop::{crop_view, CropPayload};
use crate::export_rendering::{render_cropped_image, SlidersPayload};
use crate::image_opening::ImageState;

// ── Tauri command ─────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn export_image(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    sliders: SlidersPayload,
    crop: CropPayload,
    settings: ExportSettings,
) -> Result<(), String> {
    println!(
        "export_image: opening save dialog ({:?}, {:?})",
        settings.format, settings.output_space
    );

    let (filter_name, extensions): (&str, &[&str]) = match settings.format {
        ExportFormat::Png => ("PNG Image", &["png"]),
        ExportFormat::Jpeg => ("JPEG Image", &["jpg", "jpeg"]),
        ExportFormat::Webp => ("WebP Image", &["webp"]),
        ExportFormat::Tiff => ("TIFF Image", &["tiff", "tif"]),
        ExportFormat::Jxl => ("JPEG XL Image", &["jxl"]),
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
        ExportFormat::Jxl => ensure_extension(path, "jxl"),
    };

    let (pixels_u16, width, height, channels, space) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        (
            Arc::clone(&img.pixels_u16),
            img.width,
            img.height,
            img.channels,
            img.space.clone(),
        )
    };

    let t = Instant::now();

    let bit_depth = settings.bit_depth();

    let _ = app.emit("export:started", ());

    let view = crop_view(&crop, width, height);
    let megapixels = view.width as f32 * view.height as f32 / 1e6;
    let plan = Arc::new(ProgressPlan::new(
        settings.encode_seconds_per_megapixel() * megapixels,
    ));
    let send = {
        let app = app.clone();
        move |event: Option<ProgressEvent>| {
            if let Some(event) = event {
                let _ = app.emit("export:progress", event);
            }
        }
    };
    send(plan.preparing());

    // The renderer is Tauri-agnostic; bridging its progress to the frontend is
    // this command's job.
    let render_plan = Arc::clone(&plan);
    let render_send = send.clone();
    let rendered = render_cropped_image(
        &pixels_u16,
        width,
        height,
        channels,
        &sliders,
        &crop,
        &space,
        settings.output_space,
        bit_depth,
        move |fraction| render_send(render_plan.rendered(fraction)),
    )
    .await?;

    println!(
        "export: GPU render done in {} ms, encoding…",
        t.elapsed().as_millis()
    );

    send(plan.encoding(settings.reports_encode_progress()));
    // Encoding is CPU-bound and can run for minutes, so it stays off the async
    // runtime's workers.
    let encode_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        encode_and_save_with_progress(
            rendered.rgb,
            rendered.width,
            rendered.height,
            &settings,
            &encode_path,
            &mut |fraction| send(plan.encoded(fraction)),
        )
    })
    .await
    .map_err(|e| e.to_string())??;

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

pub(crate) fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().map(|e| e.to_ascii_lowercase()) != Some(ext.into()) {
        path.with_extension(ext)
    } else {
        path
    }
}
