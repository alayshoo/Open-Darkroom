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
use crate::export_size::{resize_rgb_with_progress, resize_seconds_per_megapixel, target_size, OutputSize};
use crate::image_opening::ImageState;
use crate::retouch::RetouchPayload;

/// Where an export is written: a folder, and a name the format's extension is
/// added to when it does not already carry it.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportDestination {
    pub folder: String,
    pub file_name: String,
    /// The name came from the system's save dialog, which has already judged
    /// it for this platform, rather than being typed for every platform.
    #[serde(default)]
    pub picked: bool,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DestinationCheck {
    pub path: String,
    /// The name the file is written under, extension included.
    pub file_name: String,
    pub exists: bool,
}

/// The crop's size as rendered, and the size it is written at.
#[derive(serde::Serialize, Clone, Copy, Debug)]
pub struct OutputDimensions {
    pub source: [u32; 2],
    pub output: [u32; 2],
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
    let path = destination_path(&destination, settings.format)?;
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
        refuse_source(&path, &img.path)?;
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
    let resizing = output != (view.width, view.height);
    // Resizing shares the encoding part of the bar, ahead of the encode itself.
    let resize_seconds = if resizing {
        resize_seconds_per_megapixel(bit_depth) * view.width as f32 * view.height as f32 / 1e6
    } else {
        0.0
    };
    let encode_seconds = settings.encode_seconds_per_megapixel() * output.0 as f32 * output.1 as f32 / 1e6;
    let resize_part = resize_seconds / (resize_seconds + encode_seconds).max(f32::EPSILON);
    let measurable = settings.reports_encode_progress();
    let plan = Arc::new(ProgressPlan::new(resize_seconds + encode_seconds));
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

    send(plan.encoding(measurable || resizing));
    // Resizing and encoding are CPU-bound and can run for minutes, so they
    // stay off the async runtime's workers.
    let encode_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rgb = resize_rgb_with_progress(
            rendered.rgb,
            (rendered.width, rendered.height),
            output,
            bit_depth,
            settings.output_space.transfer(),
            &mut |fraction| send(plan.encoded(fraction * resize_part)),
        )?;
        if resizing && !measurable {
            send(plan.encoding(false));
        }
        encode_and_save_with_progress(
            rgb,
            output.0,
            output.1,
            &settings,
            exif.as_ref(),
            &encode_path,
            &mut |fraction| send(plan.encoded(resize_part + fraction * (1.0 - resize_part))),
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
pub(crate) fn export_defaults(state: tauri::State<'_, ImageState>) -> Option<ExportDestination> {
    let guard = state.lock().unwrap();
    let path = &guard.as_ref()?.path;
    Some(ExportDestination {
        folder: path.parent()?.to_string_lossy().into_owned(),
        file_name: default_stem(path),
        picked: false,
    })
}

/// The size the open image's crop renders at, and the size `size` writes it at.
#[tauri::command]
pub(crate) fn export_output_size(
    state: tauri::State<'_, ImageState>,
    crop: CropPayload,
    size: OutputSize,
) -> Option<OutputDimensions> {
    let guard = state.lock().unwrap();
    let img = guard.as_ref()?;
    let view = crop_view(&crop, img.width, img.height);
    let output = target_size(&size, view.width, view.height);
    Some(OutputDimensions { source: [view.width, view.height], output: [output.0, output.1] })
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
    let stem = source.as_deref().map_or("Untitled".into(), default_stem);

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
        picked: true,
    }))
}

/// The file an export would write, and whether one is already there.
#[tauri::command]
pub(crate) fn check_export_destination(
    state: tauri::State<'_, ImageState>,
    destination: ExportDestination,
    format: ExportFormat,
) -> Result<DestinationCheck, String> {
    let path = destination_path(&destination, format)?;
    if let Some(img) = state.lock().unwrap().as_ref() {
        refuse_source(&path, &img.path)?;
    }
    Ok(DestinationCheck {
        exists: path.exists(),
        file_name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// The characters no file name may hold on any of the platforms the app runs on.
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

fn is_export_extension(ext: &str) -> bool {
    ExportFormat::ALL.iter().any(|f| f.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// A name's stem and the extension it is written with: one the format uses is
/// kept as given, another format's is swapped for this one's, and anything
/// else stays part of the name.
fn split_extension(file_name: &str, format: ExportFormat) -> (&str, &str) {
    let name = file_name.trim();
    match name.rsplit_once('.') {
        Some((stem, ext)) if format.extensions().iter().any(|e| e.eq_ignore_ascii_case(ext)) => (stem.trim_end(), ext),
        Some((stem, ext)) if is_export_extension(ext) => (stem.trim_end(), format.extension()),
        _ => (name, format.extension()),
    }
}

/// A typed name, checked as safe to write on every platform, with the
/// format's extension.
pub fn export_file_name(file_name: &str, format: ExportFormat) -> Result<String, String> {
    let (stem, ext) = split_extension(file_name, format);
    if stem.is_empty() || stem.chars().all(|c| c == '.') {
        return Err("The file needs a name".to_string());
    }
    if let Some(c) = stem.chars().find(|c| FORBIDDEN.contains(c) || c.is_control()) {
        return Err(format!("A file name cannot contain {c:?}"));
    }
    if stem.ends_with('.') {
        return Err("A file name cannot end with a full stop".to_string());
    }
    let device = stem.split('.').next().unwrap_or_default().trim_end();
    if reserved_on_windows(device) {
        return Err(format!("{device} is a name Windows keeps for itself"));
    }
    Ok(format!("{stem}.{ext}"))
}

/// The device names Windows will not give a file, whatever its extension.
fn reserved_on_windows(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    if matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(port) = name.strip_prefix("COM").or_else(|| name.strip_prefix("LPT")) else {
        return false;
    };
    let mut chars = port.chars();
    matches!((chars.next(), chars.next()), (Some('0'..='9' | '¹' | '²' | '³'), None))
}

pub fn destination_path(destination: &ExportDestination, format: ExportFormat) -> Result<PathBuf, String> {
    let folder = Path::new(&destination.folder);
    if folder.as_os_str().is_empty() {
        return Err("Choose a folder to export to".to_string());
    }
    if !folder.is_dir() {
        return Err("The folder no longer exists".to_string());
    }
    let name = if destination.picked {
        match split_extension(&destination.file_name, format) {
            ("", _) => return Err("The file needs a name".to_string()),
            (stem, ext) => format!("{stem}.{ext}"),
        }
    } else {
        export_file_name(&destination.file_name, format)?
    };
    Ok(folder.join(name))
}

/// The name an export of `source` starts out with: its own, unless an export
/// could be written over the source itself.
pub fn default_stem(source: &Path) -> String {
    let stem = source.file_stem().map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());
    match source.extension().and_then(|e| e.to_str()) {
        Some(ext) if is_export_extension(ext) => format!("{stem}-edit"),
        _ => stem,
    }
}

/// An export is never written over the file being edited.
pub fn refuse_source(path: &Path, source: &Path) -> Result<(), String> {
    if same_file(path, source) {
        Err("That is the image being edited; choose another name".to_string())
    } else {
        Ok(())
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!((a.metadata(), b.metadata()), (Ok(a), Ok(b)) if a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(not(unix))]
    {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
    }
}

pub(crate) fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().map(|e| e.to_ascii_lowercase()) != Some(ext.into()) {
        path.with_extension(ext)
    } else {
        path
    }
}
