// src-tauri/src/export.rs
//
// Tauri command handlers for image export.
// Orchestrates: GPU render (export_rendering) → resize (export_size) → encode (export_encoding).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

use crate::crop::{crop_view, CropPayload};
use crate::exif_writing::exif_tags;
use crate::export_encoding::{encode_and_save_with_progress, ExportFormat, ExportSettings};
use crate::export_progress::{ProgressEvent, ProgressPlan};
use crate::export_rendering::{render_cropped_image, SlidersPayload};
use crate::export_size::{resize_rgb, target_size, OutputSize};
use crate::image_opening::ImageState;
use crate::retouch::RetouchPayload;

/// Where an export is written: a folder, and a name without its extension.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportDestination {
    pub folder: String,
    pub file_name: String,
}

/// Where the open image came from, which is where its export starts out.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportDefaults {
    pub folder: String,
    pub file_name: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DestinationCheck {
    pub path: String,
    pub exists: bool,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn export_image(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    sliders: SlidersPayload,
    crop: CropPayload,
    retouch: Vec<RetouchPayload>,
    settings: ExportSettings,
    destination: ExportDestination,
    size: OutputSize,
    metadata: bool,
) -> Result<(), String> {
    let path = destination_path(&destination.folder, &destination.file_name, settings.format)?;
    println!(
        "export_image: {:?}, {:?}, {:?} → {}",
        settings.format,
        settings.output_space,
        size,
        path.display()
    );

    let (pixels_u16, width, height, channels, space, exif) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        (
            Arc::clone(&img.pixels_u16),
            img.width,
            img.height,
            img.channels,
            img.space.clone(),
            metadata.then(|| exif_tags(&img.metadata)),
        )
    };

    let t = Instant::now();

    let bit_depth = settings.bit_depth();

    let _ = app.emit("export:started", ());

    let view = crop_view(&crop, width, height);
    let output = target_size(&size, view.width, view.height);
    let megapixels = output.0 as f32 * output.1 as f32 / 1e6;
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
        &retouch,
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
    // Resizing and encoding are CPU-bound and can run for minutes, so they
    // stay off the async runtime's workers.
    let encode_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rgb = resize_rgb(
            rendered.rgb,
            (rendered.width, rendered.height),
            output,
            bit_depth,
            settings.output_space.transfer(),
        )?;
        encode_and_save_with_progress(
            rgb,
            output.0,
            output.1,
            &settings,
            exif.as_ref(),
            &encode_path,
            &mut |fraction| send(plan.encoded(fraction)),
        )
    })
    .await
    .map_err(|e| e.to_string())??;

    println!(
        "export: saved {} × {} → {} in {} ms total",
        output.0,
        output.1,
        path.display(),
        t.elapsed().as_millis(),
    );

    Ok(())
}

/// The folder and name the open image's export starts out with.
#[tauri::command]
pub(crate) fn export_defaults(state: tauri::State<'_, ImageState>) -> Option<ExportDefaults> {
    let guard = state.lock().unwrap();
    let path = &guard.as_ref()?.path;
    Some(ExportDefaults {
        folder: path.parent()?.to_string_lossy().into_owned(),
        file_name: path.file_stem()?.to_string_lossy().into_owned(),
    })
}

/// None when the picker is dismissed.
#[tauri::command]
pub(crate) async fn pick_export_folder(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    let mut dialog = app.dialog().file().set_title("Export to folder");
    if let Some(start) = start.filter(|s| Path::new(s).is_dir()) {
        dialog = dialog.set_directory(start);
    }
    let folder = dialog.blocking_pick_folder()?.into_path().ok()?;
    Some(folder.to_string_lossy().into_owned())
}

/// The save dialog, starting in `folder` when it still exists and in the open
/// image's own folder otherwise. None when it is dismissed.
#[tauri::command]
pub(crate) async fn pick_export_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    format: ExportFormat,
    folder: Option<String>,
) -> Result<Option<ExportDestination>, String> {
    let source = state.lock().unwrap().as_ref().map(|img| img.path.clone());
    let start = folder
        .map(PathBuf::from)
        .filter(|f| f.is_dir())
        .or_else(|| source.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf));
    let stem = source
        .as_ref()
        .and_then(|p| p.file_stem())
        .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());

    let mut dialog = app
        .dialog()
        .file()
        .add_filter(format!("{} Image", format.label()), format.extensions())
        .set_file_name(format!("{stem}.{}", format.extension()));
    if let Some(start) = start {
        dialog = dialog.set_directory(start);
    }
    let Some(picked) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let folder = path.parent().ok_or("The file has no folder")?;
    let file_name = path.file_name().ok_or("The file has no name")?;
    Ok(Some(ExportDestination {
        folder: folder.to_string_lossy().into_owned(),
        file_name: file_name.to_string_lossy().into_owned(),
    }))
}

/// The file an export would write, and whether one is already there.
#[tauri::command]
pub(crate) fn check_export_destination(
    folder: String,
    file_name: String,
    format: ExportFormat,
) -> Result<DestinationCheck, String> {
    let path = destination_path(&folder, &file_name, format)?;
    Ok(DestinationCheck {
        exists: path.exists(),
        path: path.to_string_lossy().into_owned(),
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// The characters no file name may hold on any of the platforms the app runs on.
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// A name without its extension, checked as safe to write on every platform.
/// An extension the format already uses is dropped rather than doubled.
pub fn export_file_name(file_name: &str, format: ExportFormat) -> Result<String, String> {
    let mut stem = file_name.trim();
    if let Some((before, ext)) = stem.rsplit_once('.') {
        if format.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            stem = before.trim_end();
        }
    }
    if stem.is_empty() || stem.chars().all(|c| c == '.') {
        return Err("The file needs a name".to_string());
    }
    if let Some(c) = stem.chars().find(|c| FORBIDDEN.contains(c) || c.is_control()) {
        return Err(format!("A file name cannot contain {c:?}"));
    }
    if stem.ends_with('.') {
        return Err("A file name cannot end with a full stop".to_string());
    }
    Ok(format!("{stem}.{}", format.extension()))
}

pub fn destination_path(folder: &str, file_name: &str, format: ExportFormat) -> Result<PathBuf, String> {
    let folder = Path::new(folder);
    if folder.as_os_str().is_empty() {
        return Err("Choose a folder to export to".to_string());
    }
    if !folder.is_dir() {
        return Err("The folder no longer exists".to_string());
    }
    Ok(folder.join(export_file_name(file_name, format)?))
}

pub(crate) fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().map(|e| e.to_ascii_lowercase()) != Some(ext.into()) {
        path.with_extension(ext)
    } else {
        path
    }
}
