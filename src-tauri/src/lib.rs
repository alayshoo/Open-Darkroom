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

use image::{DynamicImage};
use serde::Serialize;
use half::f16;
use std::time::Instant;

#[derive(Serialize)]
pub struct ImagePayload {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,  // raw bytes of RGBA f16 data, length = width*height*4*2
}

use fast_image_resize as fir;
use fast_image_resize::images::Image;

fn downscale_to_2048(img: DynamicImage) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let max_edge = w.max(h);
    if max_edge <= 2048 {
        return img;
    }
    let scale = 2048.0 / max_edge as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;

    let rgba = img.to_rgba8();
    let src_image = Image::from_vec_u8(
        w.try_into().unwrap(),
        h.try_into().unwrap(),
        rgba.into_raw(),
        fir::PixelType::U8x4,
    )
    .unwrap();

    let mut dst_image = Image::new(
        new_w.try_into().unwrap(),
        new_h.try_into().unwrap(),
        fir::PixelType::U8x4,
    );

    let mut resizer = fir::Resizer::new();
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            &fir::ResizeOptions::new().resize_alg(
                fir::ResizeAlg::Convolution(fir::FilterType::Bilinear),
            ),
        )
        .unwrap();

    let buf = dst_image.into_vec();
    DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(new_w, new_h, buf).unwrap(),
    )
}

fn build_srgb_to_linear_lut() -> [f16; 256] {
  let mut lut = [f16::ZERO; 256];
  for i in 0..256 {
      let s = i as f32 / 255.0;
      let linear = if s <= 0.04045 {
          s / 12.92
      } else {
          ((s + 0.055) / 1.055_f32).powf(2.4)
      };
      lut[i] = f16::from_f32(linear);
  }
  lut
}

use tauri::ipc::Response;   // Avoid JSON serialization
use rayon::prelude::*;

#[tauri::command]
async fn open_image_file(app: tauri::AppHandle) -> Result<Response, String> {
    
    println!("open_image_file: started");

    // Open native file dialog on the Rust side
    let path = app
        .dialog()
        .file()
        .add_filter("Images", &["jpg", "jpeg", "png"])
        .blocking_pick_file()
        .ok_or("No file selected")?
        .into_path()
        .map_err(|e| e.to_string())?;

    let overall_start = Instant::now();

    // Decode the image
    let img = image::open(&path)
        .map_err(|e| format!("Failed to open image: {e}"))?;

    // Downscale if needed
    let img = downscale_to_2048(img);

    // Convert to RGBA8 and linearize
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let raw = rgba.as_raw(); // &[u8], length = w*h*4

    let lut = build_srgb_to_linear_lut();
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 8]; // 4 channels × 2 bytes

    pixels
        .par_chunks_exact_mut(8)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            let r = lut[raw[src] as usize];
            let g = lut[raw[src + 1] as usize];
            let b = lut[raw[src + 2] as usize];
            let a = lut[raw[src + 3] as usize];
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

    // Create package to send to the frontend
    let mut payload = Vec::with_capacity(8 + pixels.len());
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&pixels);


    let total_ms = overall_start.elapsed().as_millis();
    println!(
        "open_image_file: total backend time = {} ms, payload size = {} bytes",
        total_ms,
        payload.len()
    );

    Ok(Response::new(payload))
}