// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

// Modules are public so the integration tests in `tests/` can drive the
// pipeline directly, without a running Tauri app.
pub mod color;
pub mod crop;
pub mod icc;
pub mod raw;
pub mod output_space;
pub mod image_opening;
pub mod metadata;
pub mod full_resolution;
pub mod export_rendering;
pub mod retouch;
pub mod shaders;
pub mod export_encoding;
pub mod export_progress;
pub mod jxl_encoding;
pub mod catalog;
pub mod thumbnails;
mod catalog_commands;
pub mod exif_writing;
pub mod export_size;
pub mod export;
mod presets;

use image_opening::ImageState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ImageState::new(None))
        .manage(catalog_commands::CatalogState::new(None))
        .setup(|app| {
            // The editor still works without a catalog, so a failure is reported rather than fatal.
            match catalog_commands::open_catalog(app.handle()) {
                Ok(catalog) => *app.state::<catalog_commands::CatalogState>().lock().unwrap() = Some(catalog),
                Err(e) => eprintln!("catalog: could not open: {e}"),
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            wrap_cursor,
            grab_cursor,
            image_opening::open_image_path,
            image_opening::crop_histograms,
            image_opening::image_metadata,
            full_resolution::full_resolution_rows,
            export::export_image,
            export::export_defaults,
            export::pick_export_folder,
            export::pick_export_file,
            export::check_export_destination,
            export::export_output_size,
            presets::export_preset,
            presets::import_presets,
            catalog_commands::catalog_load,
            catalog_commands::catalog_create_library,
            catalog_commands::catalog_rename_library,
            catalog_commands::catalog_delete_library,
            catalog_commands::catalog_create_roll,
            catalog_commands::catalog_update_roll,
            catalog_commands::catalog_delete_roll,
            catalog_commands::catalog_create_stock,
            catalog_commands::catalog_update_stock,
            catalog_commands::catalog_delete_stock,
            catalog_commands::catalog_create_gear,
            catalog_commands::catalog_update_gear,
            catalog_commands::catalog_delete_gear,
            catalog_commands::catalog_import_files,
            catalog_commands::catalog_import_folder,
            catalog_commands::catalog_open_file,
            catalog_commands::catalog_move_images,
            catalog_commands::catalog_remove_images,
            catalog_commands::catalog_set_frame_info,
            catalog_commands::catalog_locate,
            catalog_commands::catalog_edit,
            catalog_commands::catalog_edit_history,
            catalog_commands::catalog_find_moved,
            catalog_commands::catalog_save_edit,
            catalog_commands::catalog_choose_photo_folder,
            catalog_commands::catalog_thumbnail,
            catalog_commands::catalog_render_thumbnail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


// =============== WRAP CURSOR ===============

#[tauri::command]
fn wrap_cursor(window: tauri::Window, x: f64, y: f64) -> Result<(), String> {
    use tauri::PhysicalPosition;

    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?;

    let w = size.width as f64;
    let h = size.height as f64;

    let pad = 2.0;

    let mut new_x = x;
    let mut new_y = y;

    if x <= pad {
        new_x = w - pad - 1.0;
    } else if x >= w - pad {
        new_x = pad + 1.0;
    }

    if y <= pad {
        new_y = h - pad - 1.0;
    } else if y >= h - pad {
        new_y = pad + 1.0;
    }

    if (new_x, new_y) != (x, y) {
        window
            .set_cursor_position(PhysicalPosition::new(new_x, new_y))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}


// =============== GRAB CURSOR ===============

#[tauri::command]
fn grab_cursor(window: tauri::Window, grab: bool) -> Result<(), String> {
    let _ = window.set_cursor_grab(grab);
    Ok(())
}