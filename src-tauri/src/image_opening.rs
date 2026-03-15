// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::Image;
use half::f16;
use rayon::prelude::*;
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use image::{ImageBuffer, Rgba};

type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

pub(crate) fn downscale_to_2048_u16(rgba: &Rgba16Image) -> Rgba16Image {
    let (w, h) = rgba.dimensions();
    let max_edge = w.max(h);
    if max_edge <= 2048 {
        return rgba.clone();
    }
    let scale = 2048.0 / max_edge as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;

    // fast_image_resize expects raw bytes even for U16x4
    let raw_bytes: Vec<u8> = rgba
        .as_raw()
        .iter()
        .flat_map(|&v| v.to_le_bytes())
        .collect();

    let src_image = Image::from_vec_u8(
        w.try_into().unwrap(),
        h.try_into().unwrap(),
        raw_bytes,
        fir::PixelType::U16x4,
    )
    .unwrap();

    let mut dst_image = Image::new(
        new_w.try_into().unwrap(),
        new_h.try_into().unwrap(),
        fir::PixelType::U16x4,
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

    let dst_bytes = dst_image.into_vec();
    let u16_vec: Vec<u16> = dst_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();

    ImageBuffer::from_raw(new_w, new_h, u16_vec).unwrap()
}

pub(crate) fn build_srgb_to_linear_lut_u16() -> Vec<f16> {
    (0..65536)
        .map(|i| {
            let s = i as f32 / 65535.0;
            let linear = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055_f32).powf(2.4)
            };
            f16::from_f32(linear)
        })
        .collect()
}

#[tauri::command]
pub(crate) async fn open_image_file(app: tauri::AppHandle) -> Result<Response, String> {
    println!("open_image_file: started");

    let path = app
        .dialog()
        .file()
        .add_filter("Images", &["jpg", "jpeg", "png"])
        .blocking_pick_file()
        .ok_or("No file selected")?
        .into_path()
        .map_err(|e| e.to_string())?;

    let overall_start = Instant::now();

    let img = image::open(&path)
        .map_err(|e| format!("Failed to open image: {e}"))?;

    let rgba16 = img.to_rgba16();

    // Downscale in u16 space
    let preview = downscale_to_2048_u16(&rgba16);
    let (width, height) = preview.dimensions();
    let raw = preview.as_raw(); // &[u16]

    // Linearize u16 sRGB → f16 linear
    let lut = build_srgb_to_linear_lut_u16();
    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 8]; // 4 × f16 = 8 bytes

    pixels
        .par_chunks_exact_mut(8)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            let r = lut[raw[src] as usize];
            let g = lut[raw[src + 1] as usize];
            let b = lut[raw[src + 2] as usize];
            let a = f16::from_f32(raw[src + 3] as f32 / 65535.0);
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

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

