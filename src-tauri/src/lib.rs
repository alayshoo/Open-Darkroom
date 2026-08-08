// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

// Modules are public so the integration tests in `tests/` can drive the
// pipeline directly, without a running Tauri app.
pub mod color;
pub mod image_opening;
pub mod export_rendering;
pub mod export_encoding;
mod export;

use image_opening::ImageState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ImageState::new(None))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            wrap_cursor,
            grab_cursor,
            image_opening::open_image_file,
            export::export_image,
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