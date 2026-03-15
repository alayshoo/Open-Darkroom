// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::Image;
use half::f16;
use image::DynamicImage;
use rayon::prelude::*;
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

pub(crate) fn downscale_to_2048(img: DynamicImage) -> DynamicImage {
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
            &fir::ResizeOptions::new()
                .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Bilinear)),
        )
        .unwrap();

    let buf = dst_image.into_vec();
    DynamicImage::ImageRgba8(image::RgbaImage::from_raw(new_w, new_h, buf).unwrap())
}

pub(crate) fn build_srgb_to_linear_lut() -> [f16; 256] {
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

#[tauri::command]
pub(crate) async fn open_image_file(app: tauri::AppHandle) -> Result<Response, String> {
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
            let a = f16::from_f32(raw[src + 3] as f32 / 255.0);
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

    // Create package to send to the frontend
    // Includes header with width + row as the first 8 bytes
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

