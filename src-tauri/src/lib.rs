// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![wrap_cursor, grap_cursor])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn wrap_cursor(window: tauri::Window, x: f64, y: f64) -> Result<(), String> {
  // you’ll also want window.inner_size() and some padding/threshold

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

#[tauri::command]
fn grap_cursor(window: tauri::Window, grab: bool) -> Result<(), String> {
    
    let _ = window.set_cursor_grab(grab);

    Ok(())
}