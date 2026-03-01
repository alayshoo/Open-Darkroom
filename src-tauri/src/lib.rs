// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            wrap_cursor, 
            grab_cursor,
            open_image_file,
        ])
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
fn grab_cursor(window: tauri::Window, grab: bool) -> Result<(), String> {
    
    let _ = window.set_cursor_grab(grab);

    Ok(())
}

// File opening logic

use tauri_plugin_dialog::DialogExt;

use image::{DynamicImage, imageops::FilterType};
use serde::Serialize;
use half::f16;

#[derive(Serialize)]
pub struct ImagePayload {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,  // raw bytes of RGBA f16 data, length = width*height*4*2
}

fn srgb_to_linear_f16(value: u8) -> f16 {
    let s = value as f32 / 255.0;
    let linear = if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055_f32).powf(2.4)
    };
    f16::from_f32(linear)
}

fn downscale_to_2048(img: DynamicImage) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let max_edge = w.max(h);
    if max_edge <= 2048 {
        return img;
    }
    let scale = 2048.0 / max_edge as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;
    img.resize(new_w, new_h, FilterType::Lanczos3)
}


use tauri::ipc::Response;   // Avoid JSON serialization

#[tauri::command]
async fn open_image_file(app: tauri::AppHandle) -> Result<Response, String> {
    // Open native file dialog on the Rust side
    let path = app
        .dialog()
        .file()
        .add_filter("Images", &["jpg", "jpeg", "png"])
        .blocking_pick_file()
        .ok_or("No file selected")?
        .into_path()
        .map_err(|e| e.to_string())?;

    // Decode the image
    let img = image::open(&path)
        .map_err(|e| format!("Failed to open image: {e}"))?;

    // Downscale if needed
    let img = downscale_to_2048(img);

    // Convert to RGBA8
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Linearize: apply inverse sRGB gamma per RGB channel, leave alpha as-is
    let pixels: Vec<u8> = rgba
        .pixels()
        .flat_map(|p| {
            let [r, g, b, a] = p.0;
            let rf = srgb_to_linear_f16(r);
            let gf = srgb_to_linear_f16(g);
            let bf = srgb_to_linear_f16(b);
            let af = f16::from_f32(a as f32 / 255.0);
            [rf, gf, bf, af]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>()
        })
        .collect();

    let mut payload = Vec::with_capacity(8 + pixels.len());
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&pixels);

    Ok(Response::new(payload))
}