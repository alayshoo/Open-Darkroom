// src-tauri/src/presets.rs
//
// Moves preset documents between the file dialogs and the disk. The format is
// the frontend's (presetDocument.ts); this side only reads and writes text.

use serde::Serialize;
use tauri_plugin_dialog::DialogExt;

use crate::export::ensure_extension;

/// Must match PRESET_EXTENSION in presetDocument.ts.
const EXTENSION: &str = "odpreset";
const FILTER_NAME: &str = "Open Darkroom Preset";

/// Far beyond any real preset, so a wrong pick is refused rather than read.
const MAX_PRESET_BYTES: u64 = 1 << 20;

#[derive(Serialize)]
pub struct PresetFile {
    /// The file's stem, for a document that carries no name of its own.
    name: String,
    contents: String,
}

/// Returns false when the dialog is dismissed.
#[tauri::command]
pub(crate) async fn export_preset(
    app: tauri::AppHandle,
    file_name: String,
    contents: String,
) -> Result<bool, String> {
    let Some(path) = app
        .dialog()
        .file()
        .add_filter(FILTER_NAME, &[EXTENSION])
        .set_file_name(format!("{file_name}.{EXTENSION}"))
        .blocking_save_file()
    else {
        return Ok(false);
    };

    let path = ensure_extension(path.into_path().map_err(|e| e.to_string())?, EXTENSION);
    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Every file picked, or none when the dialog is dismissed.
#[tauri::command]
pub(crate) async fn import_presets(app: tauri::AppHandle) -> Result<Vec<PresetFile>, String> {
    let Some(paths) = app
        .dialog()
        .file()
        .add_filter(FILTER_NAME, &[EXTENSION])
        .blocking_pick_files()
    else {
        return Ok(Vec::new());
    };

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.into_path().map_err(|e| e.to_string())?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size > MAX_PRESET_BYTES {
            return Err(format!("{} is too large to be a preset", path.display()));
        }
        let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        files.push(PresetFile { name, contents });
    }
    Ok(files)
}
