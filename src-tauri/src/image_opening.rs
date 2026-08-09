// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::{Image, ImageRef};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use image::{ImageBuffer, Rgba};

use crate::color::{
    build_resample_lut, build_srgb_to_linear_lut_u16, linear_u16_to_f16_bytes, linearize_rgba_u16,
    srgb_to_linear_rgba_u16, LINEAR_BYTES_PER_PIXEL,
};

pub type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

/// Longest edge of the preview handed to the frontend.
pub const PREVIEW_MAX_EDGE: u32 = 2048;

/// Bins in each per-channel histogram.
pub const HIST_BINS: usize = 256;

/// Bytes preceding the pixel data in the frontend payload: preview width and
/// height, then full-resolution width and height, u32 LE. The full dimensions
/// are what anchors a slider specified in image pixels to a downscaled preview.
pub const PAYLOAD_HEADER_BYTES: usize = 16;

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OriginalImage {
    /// Full-resolution RGBA u16 pixels. Behind an `Arc` so export can take a
    /// reference-counted handle instead of copying the whole buffer.
    pub pixels_u16: Arc<Vec<u16>>,
    pub width: u32,
    pub height: u32,
}

pub type ImageState = Mutex<Option<OriginalImage>>;

// ── Derived image data ────────────────────────────────────────────────────────

/// Per-channel histograms of the full-resolution sRGB image.
pub struct Histograms {
    pub r: [u32; HIST_BINS],
    pub g: [u32; HIST_BINS],
    pub b: [u32; HIST_BINS],
}

/// Everything derived from a decoded image that the frontend needs, with no
/// Tauri or filesystem involvement — this is the unit the tests exercise.
pub struct PreparedImage {
    /// Linearised preview pixels: RGBA f16 little-endian, 8 bytes per pixel.
    pub preview_pixels: Vec<u8>,
    pub preview_width: u32,
    pub preview_height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub histograms: Histograms,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Downscale so the longest edge is at most `max_edge`, preserving aspect ratio.
/// Images already within the limit are returned unchanged.
///
/// Resampling happens in u16 space so the preview keeps the source precision.
/// `max_edge` must be greater than zero.
pub fn downscale_to_max_edge(rgba: &Rgba16Image, max_edge: u32) -> Rgba16Image {
    assert!(max_edge > 0, "max_edge must be non-zero");

    let (w, h) = rgba.dimensions();
    if w.max(h) <= max_edge {
        return rgba.clone();
    }

    let scale = max_edge as f32 / w.max(h) as f32;
    let new_w = ((w as f32 * scale).round() as u32).max(1);
    let new_h = ((h as f32 * scale).round() as u32).max(1);

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

/// R/G/B histograms of the full-resolution image. u16 sRGB values (0–65535) are
/// mapped to 8-bit bins (0–255) via `>> 8`.
pub fn compute_histograms(rgba16: &Rgba16Image) -> Histograms {
    let mut r = [0u32; HIST_BINS];
    let mut g = [0u32; HIST_BINS];
    let mut b = [0u32; HIST_BINS];

    for px in rgba16.as_raw().chunks_exact(4) {
        r[(px[0] >> 8) as usize] += 1;
        g[(px[1] >> 8) as usize] += 1;
        b[(px[2] >> 8) as usize] += 1;
    }

    Histograms { r, g, b }
}

/// Build the preview texture and histograms for a decoded image.
///
/// Borrows rather than consumes, so the caller can move the full-resolution
/// buffer into the shared state afterwards without cloning it.
pub fn prepare_image(rgba16: &Rgba16Image) -> PreparedImage {
    let (full_width, full_height) = rgba16.dimensions();

    // Resampling happens in linear light, so the image is gamma-decoded before
    // it is downscaled rather than after.
    //
    // Averaging sRGB code values averages the encoding, not the light. Reduce a
    // black-and-white checkerboard to a single pixel that way and it lands on
    // sRGB 128 — linear 0.21 — where the light those pixels stand for is linear
    // 0.50, sRGB 188. That is most of a stop, and it goes missing exactly in the
    // fine texture the preview exists to predict: foliage, hair, grain,
    // speculars. The live histogram reads the same preview, so it drifted too,
    // and the export renders full-resolution and never resampled at all — so
    // the two disagreed by more than resolution.
    //
    // Images already inside the cap are not resampled, and take the direct path
    // so their bytes stay identical to what the export path would upload.
    let (preview_width, preview_height, preview_pixels) =
        if full_width.max(full_height) > PREVIEW_MAX_EDGE {
            let linear: Rgba16Image = ImageBuffer::from_raw(
                full_width,
                full_height,
                srgb_to_linear_rgba_u16(rgba16.as_raw(), &build_resample_lut()),
            )
            .expect("a linearised buffer keeps the dimensions of its source");

            let preview = downscale_to_max_edge(&linear, PREVIEW_MAX_EDGE);
            let (w, h) = preview.dimensions();
            (w, h, linear_u16_to_f16_bytes(preview.as_raw()))
        } else {
            let lut = build_srgb_to_linear_lut_u16();
            (
                full_width,
                full_height,
                linearize_rgba_u16(rgba16.as_raw(), &lut),
            )
        };

    // Histograms come from the full-resolution image, not the preview.
    let histograms = compute_histograms(rgba16);

    PreparedImage {
        preview_pixels,
        preview_width,
        preview_height,
        full_width,
        full_height,
        histograms,
    }
}

/// Serialise a [`PreparedImage`] into the byte layout `openImage.ts` parses:
///
/// | offset | size          | contents                          |
/// |--------|---------------|-----------------------------------|
/// | 0      | 4             | preview width, u32 LE             |
/// | 4      | 4             | preview height, u32 LE            |
/// | 8      | w × h × 8     | RGBA f16 LE preview pixels        |
/// | …      | 3 × 256 × 4   | R, G, B histogram bins, u32 LE    |
pub fn build_payload(prepared: &PreparedImage) -> Vec<u8> {
    let hist_bytes = 3 * HIST_BINS * 4;
    let mut payload =
        Vec::with_capacity(PAYLOAD_HEADER_BYTES + prepared.preview_pixels.len() + hist_bytes);

    payload.extend_from_slice(&prepared.preview_width.to_le_bytes());
    payload.extend_from_slice(&prepared.preview_height.to_le_bytes());
    payload.extend_from_slice(&prepared.full_width.to_le_bytes());
    payload.extend_from_slice(&prepared.full_height.to_le_bytes());
    payload.extend_from_slice(&prepared.preview_pixels);

    let h = &prepared.histograms;
    for &v in h.r.iter().chain(h.g.iter()).chain(h.b.iter()) {
        payload.extend_from_slice(&v.to_le_bytes());
    }

    payload
}

/// Expected payload length for a preview of the given dimensions.
pub fn payload_len_for(preview_width: u32, preview_height: u32) -> usize {
    PAYLOAD_HEADER_BYTES
        + (preview_width as usize) * (preview_height as usize) * LINEAR_BYTES_PER_PIXEL
        + 3 * HIST_BINS * 4
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

    let prepared = prepare_image(&rgba16);
    let (full_width, full_height) = (prepared.full_width, prepared.full_height);

    // Store the original full-resolution image for export. This happens after
    // `prepare_image` so the buffer can be moved into the state rather than
    // cloned — preparation only needed to borrow it.
    {
        let mut guard = state.lock().unwrap();
        *guard = Some(OriginalImage {
            pixels_u16: Arc::new(rgba16.into_raw()),
            width: full_width,
            height: full_height,
        });
    }

    let payload = build_payload(&prepared);

    println!(
        "open_image_file: total backend time = {} ms, payload size = {} bytes",
        overall_start.elapsed().as_millis(),
        payload.len()
    );

    Ok(Response::new(payload))
}
