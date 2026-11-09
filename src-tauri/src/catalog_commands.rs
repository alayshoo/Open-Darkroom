// src-tauri/src/catalog_commands.rs
//
// The catalog and thumbnails as Tauri commands. The catalog is opened once,
// in the app's data directory, when the app starts.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Response;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use crate::catalog::{
    find_moved, fingerprint, folder_images, prepare_import, search_roots, Catalog, FilmStock, Gear, GearFields, ImageRecord, Place,
    Roll, RollGear, Skipped, Snapshot, StockFields, Target,
};
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
    stock_id: Option<i64>,
    gear: RollGear,
) -> Result<(), String> {
    with_catalog(&state, |c| c.update_roll(id, &name, &info, stock_id, &gear))
}

#[tauri::command]
pub(crate) fn catalog_create_gear(state: tauri::State<'_, CatalogState>, gear: GearFields) -> Result<Gear, String> {
    with_catalog(&state, |c| c.create_gear(&gear))
}

#[tauri::command]
pub(crate) fn catalog_update_gear(state: tauri::State<'_, CatalogState>, id: i64, gear: GearFields) -> Result<(), String> {
    with_catalog(&state, |c| c.update_gear(id, &gear))
}

#[tauri::command]
pub(crate) fn catalog_delete_gear(state: tauri::State<'_, CatalogState>, id: i64) -> Result<(), String> {
    with_catalog(&state, |c| c.delete_gear(id))
}

#[tauri::command]
pub(crate) fn catalog_create_stock(state: tauri::State<'_, CatalogState>, stock: StockFields) -> Result<FilmStock, String> {
    with_catalog(&state, |c| c.create_film_stock(&stock))
}

#[tauri::command]
pub(crate) fn catalog_update_stock(state: tauri::State<'_, CatalogState>, id: i64, stock: StockFields) -> Result<(), String> {
    with_catalog(&state, |c| c.update_film_stock(id, &stock))
}

#[tauri::command]
pub(crate) fn catalog_delete_stock(state: tauri::State<'_, CatalogState>, id: i64) -> Result<(), String> {
    with_catalog(&state, |c| c.delete_film_stock(id))
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
    skipped: Vec<Skipped>,
}

impl Imported {
    fn nothing() -> Self {
        Self { roll: None, images: Vec::new(), skipped: Vec::new() }
    }
}

fn pick_images(app: &tauri::AppHandle) -> Option<Vec<PathBuf>> {
    let extensions = image_extensions();
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
    let picked = app.dialog().file().add_filter("Images", &extensions).blocking_pick_files()?;
    Some(picked.into_iter().filter_map(|p| p.into_path().ok()).collect())
}

/// Imports `paths`, leaving the catalog free while the files are read and copied.
async fn import_paths(state: &CatalogState, paths: Vec<PathBuf>, target: Target, copy: bool) -> Result<Imported, String> {
    let plan = with_catalog(state, |c| c.plan_import(target, copy))?;
    let prepared = tauri::async_runtime::spawn_blocking(move || prepare_import(plan, &paths))
        .await
        .map_err(|e| e.to_string())??;
    let (roll, import) = with_catalog(state, |c| c.record_import(prepared))?;
    Ok(Imported { roll, images: import.images, skipped: import.skipped })
}

/// Picks files and adds them to a library or roll. Nothing is imported when
/// the picker is dismissed, and files that cannot be read are skipped.
#[tauri::command]
pub(crate) async fn catalog_import_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    place: Place,
    copy: bool,
) -> Result<Imported, String> {
    let Some(paths) = pick_images(&app) else {
        return Ok(Imported::nothing());
    };
    import_paths(&state, paths, Target::Place(place), copy).await
}

/// Picks a folder and makes a roll of the images directly inside it, named
/// after it and numbered in file name order. None when the picker is dismissed.
#[tauri::command]
pub(crate) async fn catalog_import_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    library_id: i64,
    copy: bool,
) -> Result<Option<Imported>, String> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    let paths = folder_images(&folder, &image_extensions())?;
    if paths.is_empty() {
        return Err(format!("There are no images in {}", folder.display()));
    }
    let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Roll".into());
    import_paths(&state, paths, Target::NewRoll { library_id, name }, copy).await.map(Some)
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
    let (_, print) = fingerprint(&path)?;
    if let Some(id) = with_catalog(&state, |c| c.find_by_fingerprint(&print))? {
        return with_catalog(&state, |c| c.image(id)).map(Some);
    }
    let mut imported = import_paths(&state, vec![path], Target::Place(place), copy).await?;
    match imported.skipped.pop() {
        Some(skipped) => Err(skipped.reason),
        None => Ok(imported.images.pop()),
    }
}

#[tauri::command]
pub(crate) fn catalog_move_images(
    state: tauri::State<'_, CatalogState>,
    ids: Vec<i64>,
    place: Place,
) -> Result<Vec<ImageRecord>, String> {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedFiles {
    /// Images found somewhere else, now pointing there.
    pub found: Vec<ImageRecord>,
    /// Images whose files are still not found.
    pub missing: Vec<i64>,
}

/// Images an automatic search has already looked for and not found. They are
/// looked for again on the next launch, or when a file is located by hand.
static NOT_FOUND: Mutex<Option<HashSet<i64>>> = Mutex::new(None);

/// Looks for the files that are not where they were, around where they were
/// and around `near`, a file just located. The catalog is free while it looks.
#[tauri::command]
pub(crate) async fn catalog_find_moved(state: tauri::State<'_, CatalogState>, near: Option<String>) -> Result<MovedFiles, String> {
    let (missing, photo_folder) = with_catalog(&state, |c| Ok((c.missing_files()?, c.photo_folder()?)))?;
    let looking: Vec<_> = {
        let mut not_found = NOT_FOUND.lock().unwrap_or_else(|e| e.into_inner());
        let not_found = not_found.get_or_insert_with(HashSet::new);
        not_found.retain(|id| missing.iter().any(|m| m.id == *id));
        match near {
            Some(_) => missing.clone(),
            None => missing.iter().filter(|m| !not_found.contains(&m.id)).cloned().collect(),
        }
    };
    if looking.is_empty() {
        return Ok(MovedFiles { found: vec![], missing: missing.iter().map(|m| m.id).collect() });
    }
    let near = near.map(PathBuf::from).and_then(|p| p.parent().map(PathBuf::from));
    let searched: Vec<i64> = looking.iter().map(|m| m.id).collect();
    let moved = tauri::async_runtime::spawn_blocking(move || {
        let roots = search_roots(&looking, near.as_deref(), photo_folder.as_deref());
        find_moved(&looking, &roots)
    })
    .await
    .map_err(|e| e.to_string())?;
    with_catalog(&state, |c| {
        let found: Vec<ImageRecord> = moved.iter().filter_map(|(id, path)| c.relink(*id, path).ok()).collect();
        let unfound = searched.into_iter().filter(|id| !found.iter().any(|f| f.id == *id));
        NOT_FOUND.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).extend(unfound);
        let missing = missing.iter().map(|m| m.id).filter(|id| !found.iter().any(|f| f.id == *id)).collect();
        Ok(MovedFiles { found, missing })
    })
}

#[tauri::command]
pub(crate) fn catalog_edit(state: tauri::State<'_, CatalogState>, id: i64) -> Result<Option<String>, String> {
    with_catalog(&state, |c| c.edit(id))
}

#[tauri::command]
pub(crate) fn catalog_edit_history(state: tauri::State<'_, CatalogState>, id: i64) -> Result<Option<String>, String> {
    with_catalog(&state, |c| c.edit_history(id))
}

#[tauri::command]
pub(crate) fn catalog_save_edit(
    state: tauri::State<'_, CatalogState>,
    id: i64,
    edit: String,
    history: Option<String>,
) -> Result<i64, String> {
    with_catalog(&state, |c| c.save_edit(id, &edit, history.as_deref()))
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
/// An image open in the editor is shrunk from the pixels already held; any
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
                    ThumbnailSource::from_pixels(&path, &pixels, w, h, channels, space)
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
    // Stored only while the id still names the same file, since a removed image's id can be reused.
    with_catalog(&catalog, |c| match c.image(id) {
        Ok(now) if now.path == record.path => std::fs::write(&target, &jpeg).map_err(|e| e.to_string()),
        _ => Err(format!("Image {id} changed while its thumbnail rendered")),
    })?;
    Ok(Response::new(jpeg))
}
