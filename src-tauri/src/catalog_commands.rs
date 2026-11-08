// src-tauri/src/catalog_commands.rs
//
// The catalog and thumbnails as Tauri commands. The catalog is opened once,
// in the app's data directory, when the app starts.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Response;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use crate::catalog::{folder_images, Catalog, ImageRecord, Place, Roll, Snapshot};
use crate::crop::CropPayload;
use crate::export_rendering::SlidersPayload;
use crate::image_opening::{image_extensions, ImageState};
use crate::retouch::RetouchPayload;
use crate::thumbnails::{cached_source, keep_source, render_thumbnail, ThumbnailSource};

pub type CatalogState = Mutex<Option<Catalog>>;

/// Returned when there is no stored thumbnail yet. Must match
/// NO_THUMBNAIL in library/catalogApi.ts.
const NO_THUMBNAIL: &str = "thumbnail:none";

pub fn open_catalog(app: &tauri::AppHandle) -> Result<Catalog, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Catalog::open(&dir)
}

fn with_catalog<T>(state: &CatalogState, f: impl FnOnce(&Catalog) -> Result<T, String>) -> Result<T, String> {
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.as_ref().ok_or("The catalog could not be opened")?)
}

#[tauri::command]
pub(crate) fn catalog_load(state: tauri::State<'_, CatalogState>) -> Result<Snapshot, String> {
    with_catalog(&state, |c| c.snapshot())
}

#[tauri::command]
pub(crate) fn catalog_create_library(state: tauri::State<'_, CatalogState>, name: String) -> Result<crate::catalog::Library, String> {
    with_catalog(&state, |c| c.create_library(&name))
}

#[tauri::command]
pub(crate) fn catalog_rename_library(state: tauri::State<'_, CatalogState>, id: i64, name: String) -> Result<(), String> {
    with_catalog(&state, |c| c.rename_library(id, &name))
}

#[tauri::command]
pub(crate) fn catalog_delete_library(state: tauri::State<'_, CatalogState>, id: i64) -> Result<(), String> {
    with_catalog(&state, |c| c.delete_library(id))
}

#[tauri::command]
pub(crate) fn catalog_create_roll(
    state: tauri::State<'_, CatalogState>,
    library_id: i64,
    name: String,
) -> Result<Roll, String> {
    with_catalog(&state, |c| c.create_roll(library_id, &name))
}

#[tauri::command]
pub(crate) fn catalog_update_roll(
    state: tauri::State<'_, CatalogState>,
    id: i64,
    name: String,
    info: Value,
) -> Result<(), String> {
    with_catalog(&state, |c| c.update_roll(id, &name, &info))
}

#[tauri::command]
pub(crate) fn catalog_delete_roll(state: tauri::State<'_, CatalogState>, id: i64) -> Result<(), String> {
    with_catalog(&state, |c| c.delete_roll(id))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    /// The roll made for a folder imported as one.
    roll: Option<Roll>,
    images: Vec<ImageRecord>,
}

fn pick_images(app: &tauri::AppHandle) -> Option<Vec<PathBuf>> {
    let extensions = image_extensions();
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
    let picked = app.dialog().file().add_filter("Images", &extensions).blocking_pick_files()?;
    Some(picked.into_iter().filter_map(|p| p.into_path().ok()).collect())
}

/// Picks files and adds them to a library or roll. Nothing is imported when
/// the picker is dismissed.
#[tauri::command]
pub(crate) async fn catalog_import_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    place: Place,
    copy: bool,
) -> Result<Imported, String> {
    let Some(paths) = pick_images(&app) else {
        return Ok(Imported { roll: None, images: Vec::new() });
    };
    let images = with_catalog(&state, |c| c.import(&paths, place, copy))?;
    Ok(Imported { roll: None, images })
}

/// Picks a folder and makes a roll of the images directly inside it, named
/// after it and numbered in file name order.
#[tauri::command]
pub(crate) async fn catalog_import_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    library_id: i64,
    copy: bool,
) -> Result<Imported, String> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(Imported { roll: None, images: Vec::new() });
    };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    let paths = folder_images(&folder, &image_extensions())?;
    if paths.is_empty() {
        return Err(format!("There are no images in {}", folder.display()));
    }
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Roll".into());
    with_catalog(&state, |c| {
        let roll = c.create_roll(library_id, &name)?;
        let images = c.import(&paths, Place { library_id, roll_id: Some(roll.id) }, copy)?;
        Ok(Imported { roll: Some(roll), images })
    })
}

/// Picks one file to open. It is added to `place` unless the catalog already
/// has it, and either way the image's id comes back; None when dismissed.
#[tauri::command]
pub(crate) async fn catalog_open_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    place: Place,
    copy: bool,
) -> Result<Option<ImageRecord>, String> {
    let extensions = image_extensions();
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
    let Some(picked) = app.dialog().file().add_filter("Images", &extensions).blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    with_catalog(&state, |c| {
        if let Some(id) = c.find_by_file(&path)? {
            return c.image(id).map(Some);
        }
        Ok(c.import(&[path], place, copy)?.pop())
    })
}

#[tauri::command]
pub(crate) fn catalog_move_images(
    state: tauri::State<'_, CatalogState>,
    ids: Vec<i64>,
    place: Place,
) -> Result<(), String> {
    with_catalog(&state, |c| c.move_images(&ids, place))
}

#[tauri::command]
pub(crate) fn catalog_remove_images(state: tauri::State<'_, CatalogState>, ids: Vec<i64>) -> Result<(), String> {
    with_catalog(&state, |c| c.remove_images(&ids))
}

#[tauri::command]
pub(crate) fn catalog_set_frame_info(state: tauri::State<'_, CatalogState>, id: i64, info: Value) -> Result<(), String> {
    with_catalog(&state, |c| c.set_frame_info(id, &info))
}

/// Asks where a missing file went. None when the picker is dismissed.
#[tauri::command]
pub(crate) async fn catalog_locate(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    id: i64,
) -> Result<Option<ImageRecord>, String> {
    let Some(picked) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    with_catalog(&state, |c| c.relink(id, &path)).map(Some)
}

#[tauri::command]
pub(crate) fn catalog_edit(state: tauri::State<'_, CatalogState>, id: i64) -> Result<Option<String>, String> {
    with_catalog(&state, |c| c.edit(id))
}

#[tauri::command]
pub(crate) fn catalog_save_edit(state: tauri::State<'_, CatalogState>, id: i64, edit: String) -> Result<i64, String> {
    with_catalog(&state, |c| c.save_edit(id, &edit))
}

/// Picks the folder copied-in images go to. None when dismissed.
#[tauri::command]
pub(crate) async fn catalog_choose_photo_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
) -> Result<Option<String>, String> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    with_catalog(&state, |c| c.set_photo_folder(&folder))?;
    Ok(Some(folder.to_string_lossy().into_owned()))
}

/// The stored thumbnail, as JPEG bytes.
#[tauri::command]
pub(crate) fn catalog_thumbnail(state: tauri::State<'_, CatalogState>, id: i64) -> Result<Response, String> {
    let path = with_catalog(&state, |c| Ok(c.thumbnail_path(id)))?;
    std::fs::read(path).map(Response::new).map_err(|_| NO_THUMBNAIL.to_string())
}

/// Renders an image's thumbnail with the edit given, stores it and returns it.
///
/// An image open in the editor is shrunk from the pixels already held. Any
/// other is decoded, and only its shrunk copy is kept.
#[tauri::command]
pub(crate) async fn catalog_render_thumbnail(
    catalog: tauri::State<'_, CatalogState>,
    image_state: tauri::State<'_, ImageState>,
    id: i64,
    sliders: SlidersPayload,
    crop: CropPayload,
    retouch: Vec<RetouchPayload>,
) -> Result<Response, String> {
    let (record, target) = with_catalog(&catalog, |c| Ok((c.image(id)?, c.thumbnail_path(id))))?;
    let path = PathBuf::from(&record.path);

    let source = match cached_source(&path) {
        Some(source) => source,
        None => {
            let open = {
                let guard = image_state.lock().unwrap();
                guard.as_ref().filter(|img| img.path == path).map(|img| {
                    (Arc::clone(&img.pixels_u16), img.width, img.height, img.channels, img.space.clone())
                })
            };
            let path = path.clone();
            let source = tauri::async_runtime::spawn_blocking(move || match open {
                Some((pixels, w, h, channels, space)) => {
                    Ok(ThumbnailSource::from_pixels(&path, &pixels, w, h, channels, space))
                }
                None => ThumbnailSource::decode(&path),
            })
            .await
            .map_err(|e| e.to_string())??;
            let source = Arc::new(source);
            keep_source(Arc::clone(&source));
            source
        }
    };

    let jpeg = render_thumbnail(&source, &sliders, &crop, &retouch).await?;
    std::fs::write(&target, &jpeg).map_err(|e| e.to_string())?;
    Ok(Response::new(jpeg))
}
