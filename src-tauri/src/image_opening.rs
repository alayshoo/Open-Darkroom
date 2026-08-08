// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::{Image, ImageRef};
use half::f16;
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use image::{ImageBuffer, Rgba};

type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OriginalImage {
    /// Full-resolution RGBA u16 pixels. Behind an `Arc` so export can take a
    /// reference-counted handle instead of copying the whole buffer.
    pub pixels_u16: Arc<Vec<u16>>,
    pub width: u32,
    pub height: u32,
}

pub type ImageState = Mutex<Option<OriginalImage>>;

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn downscale_to_2048_u16(rgba: &Rgba16Image) -> Rgba16Image {
    let (w, h) = rgba.dimensions();
    let max_edge = w.max(h);
    if max_edge <= 2048 {
        return rgba.clone();
    }
    let scale = 2048.0 / max_edge as f32;
    let new_w = (w as f32 * scale).round() as u32;
    let new_h = (h as f32 * scale).round() as u32;

    let src_image = ImageRef::new(
        w,
        h,
        bytemuck::cast_slice(rgba.as_raw()),
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

/// sRGB u16 → linear f16 lookup table (used for both preview upload and export).
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

// ── Tauri command ─────────────────────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn open_image_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
) -> Result<Response, String> {
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

    let img = image::open(&path).map_err(|e| format!("Failed to open image: {e}"))?;

    // `into_rgba16` consumes the DynamicImage, so the decoded source buffer is
    // released as soon as the conversion is done rather than idling.
    let rgba16 = img.into_rgba16();
    let (full_width, full_height) = rgba16.dimensions();

    // Downscale in u16 space for the preview sent to the frontend
    let preview = downscale_to_2048_u16(&rgba16);
    let (width, height) = preview.dimensions();
    let raw = preview.as_raw();

    // Linearize u16 sRGB → f16 linear for the rgba16float GPU texture
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

    // Compute R/G/B histograms (256 bins) from original full-resolution image.
    // Map u16 sRGB values (0-65535) to 8-bit bins (0-255) via >> 8.
    let mut hist_r = [0u32; 256];
    let mut hist_g = [0u32; 256];
    let mut hist_b = [0u32; 256];
    for chunk in rgba16.as_raw().chunks_exact(4) {
        hist_r[(chunk[0] >> 8) as usize] += 1;
        hist_g[(chunk[1] >> 8) as usize] += 1;
        hist_b[(chunk[2] >> 8) as usize] += 1;
    }

    // Store the original full-resolution image for export. This happens last so
    // the buffer can be moved into the state rather than cloned — everything
    // above only needed to borrow it.
    {
        let mut guard = state.lock().unwrap();
        *guard = Some(OriginalImage {
            pixels_u16: Arc::new(rgba16.into_raw()),
            width: full_width,
            height: full_height,
        });
    }

    let mut payload = Vec::with_capacity(8 + pixels.len() + 3072);
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend_from_slice(&pixels);
    for &v in hist_r.iter().chain(hist_g.iter()).chain(hist_b.iter()) {
        payload.extend_from_slice(&v.to_le_bytes());
    }

    let total_ms = overall_start.elapsed().as_millis();
    println!(
        "open_image_file: total backend time = {} ms, payload size = {} bytes",
        total_ms,
        payload.len()
    );

    Ok(Response::new(payload))
}
