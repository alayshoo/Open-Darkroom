// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::{Image, ImageRef};
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use image::{ImageBuffer, Rgba};

use crate::color::{
    build_resample_lut, build_srgb_to_linear_lut_u16, linear_u16_to_f16_bytes, linearize_u16,
    srgb_to_linear_u16, LINEAR_BYTES_PER_PIXEL,
};

/// The four-channel decode shape, produced whenever the source declares alpha.
pub type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

/// Longest edge of the preview handed to the frontend.
pub const PREVIEW_MAX_EDGE: u32 = 2048;

/// Longest edge of the overview handed to the frontend.
///
/// The overview always covers the whole frame, whatever the preview shows, so
/// the live histogram describes the image rather than the viewport. A 256-bin
/// distribution needs no more than this, and the small texture keeps the
/// recompute cheap on every slider move.
pub const OVERVIEW_MAX_EDGE: u32 = 512;

/// Bins in each per-channel histogram.
pub const HIST_BINS: usize = 256;

/// Pixels one worker bins before its partial counts are merged. Splitting any
/// finer would cost more in carrying the 3 KB of counters around than it saves.
const HIST_BLOCK_PIXELS: usize = 1 << 16;

/// Bytes preceding the pixel data in the frontend payload: preview width and
/// height, full-resolution width and height, then overview width and height,
/// u32 LE. The full dimensions are what anchors a slider specified in image
/// pixels to a downscaled preview.
pub const PAYLOAD_HEADER_BYTES: usize = 24;

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OriginalImage {
    /// Full-resolution interleaved u16 pixels. Behind an `Arc` so export can
    /// take a reference-counted handle instead of copying the whole buffer.
    pub pixels_u16: Arc<Vec<u16>>,
    pub width: u32,
    pub height: u32,
    /// Samples per pixel: 3 when the source declared no alpha channel, 4 when
    /// it did. Sources without alpha are stored three-wide, which is a quarter
    /// less memory held for as long as the image is open.
    pub channels: usize,
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
    pub overview_pixels: Vec<u8>,
    pub overview_width: u32,
    pub overview_height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub histograms: Histograms,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Downscale so the longest edge is at most `max_edge`, preserving aspect ratio,
/// and return the pixels with their new dimensions. Images already within the
/// limit are returned unchanged.
///
/// Resampling happens in u16 space so the preview keeps the source precision.
/// `channels` is 3 or 4; `max_edge` must be greater than zero.
pub fn downscale_to_max_edge(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    max_edge: u32,
) -> (Vec<u16>, u32, u32) {
    assert!(max_edge > 0, "max_edge must be non-zero");
    assert!(
        channels == 3 || channels == 4,
        "channels must be 3 (RGB) or 4 (RGBA), got {channels}"
    );

    if width.max(height) <= max_edge {
        return (src.to_vec(), width, height);
    }

    let scale = max_edge as f32 / width.max(height) as f32;
    let new_w = ((width as f32 * scale).round() as u32).max(1);
    let new_h = ((height as f32 * scale).round() as u32).max(1);

    let pixel_type = if channels == 4 {
        fir::PixelType::U16x4
    } else {
        fir::PixelType::U16x3
    };

    let src_image = ImageRef::new(width, height, bytemuck::cast_slice(src), pixel_type).unwrap();

    let mut dst_image = Image::new(
        new_w.try_into().unwrap(),
        new_h.try_into().unwrap(),
        pixel_type,
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

    (u16_vec, new_w, new_h)
}

/// R/G/B histograms of the full-resolution image. u16 sRGB values (0–65535) are
/// mapped to 8-bit bins (0–255) via `>> 8`. Alpha, where present, is not binned.
///
/// Each worker bins into its own set of counters and the partials are summed at
/// the end, so the hot loop needs no atomics.
pub fn compute_histograms(src: &[u16], channels: usize) -> Histograms {
    let empty = || ([0u32; HIST_BINS], [0u32; HIST_BINS], [0u32; HIST_BINS]);

    let (r, g, b) = src
        .par_chunks(HIST_BLOCK_PIXELS * channels)
        .map(|block| {
            let mut bins = empty();
            for px in block.chunks_exact(channels) {
                bins.0[(px[0] >> 8) as usize] += 1;
                bins.1[(px[1] >> 8) as usize] += 1;
                bins.2[(px[2] >> 8) as usize] += 1;
            }
            bins
        })
        .reduce(empty, |mut acc, part| {
            for i in 0..HIST_BINS {
                acc.0[i] += part.0[i];
                acc.1[i] += part.1[i];
                acc.2[i] += part.2[i];
            }
            acc
        });

    Histograms { r, g, b }
}

/// Build the preview and overview textures and the histograms for a decoded
/// image.
///
/// Borrows rather than consumes, so the caller can move the full-resolution
/// buffer into the shared state afterwards without cloning it. `channels` is 3
/// for a source with no alpha channel and 4 for one with; both textures are RGBA
/// either way, since that is what the texture format requires.
pub fn prepare_image(
    src: &[u16],
    full_width: u32,
    full_height: u32,
    channels: usize,
) -> PreparedImage {
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
    //
    // A source small enough to skip both downscales needs no linear u16
    // intermediate at all; anything larger is resampled at least once, and the
    // decoded buffer is shared between the two.
    let long_edge = full_width.max(full_height);
    let linear = (long_edge > OVERVIEW_MAX_EDGE)
        .then(|| srgb_to_linear_u16(src, channels, &build_resample_lut()));

    let (preview_width, preview_height, preview_pixels) = match &linear {
        Some(linear) if long_edge > PREVIEW_MAX_EDGE => {
            let (preview, w, h) =
                downscale_to_max_edge(linear, full_width, full_height, channels, PREVIEW_MAX_EDGE);
            (w, h, linear_u16_to_f16_bytes(&preview, channels))
        }
        _ => {
            let lut = build_srgb_to_linear_lut_u16();
            (full_width, full_height, linearize_u16(src, channels, &lut))
        }
    };

    // The overview always comes off the full frame, never off the preview, so it
    // keeps describing the whole image once the preview becomes a crop tile.
    let (overview_width, overview_height, overview_pixels) = match &linear {
        Some(linear) => {
            let (overview, w, h) =
                downscale_to_max_edge(linear, full_width, full_height, channels, OVERVIEW_MAX_EDGE);
            (w, h, linear_u16_to_f16_bytes(&overview, channels))
        }
        None => (preview_width, preview_height, preview_pixels.clone()),
    };

    // Histograms come from the full-resolution image, not the preview.
    let histograms = compute_histograms(src, channels);

    PreparedImage {
        preview_pixels,
        preview_width,
        preview_height,
        overview_pixels,
        overview_width,
        overview_height,
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
/// | 8      | 4             | full width, u32 LE                |
/// | 12     | 4             | full height, u32 LE               |
/// | 16     | 4             | overview width, u32 LE            |
/// | 20     | 4             | overview height, u32 LE           |
/// | 24     | w × h × 8     | RGBA f16 LE preview pixels        |
/// | …      | ow × oh × 8   | RGBA f16 LE overview pixels       |
/// | …      | 3 × 256 × 4   | R, G, B histogram bins, u32 LE    |
pub fn build_payload(prepared: &PreparedImage) -> Vec<u8> {
    let hist_bytes = 3 * HIST_BINS * 4;
    let mut payload = Vec::with_capacity(
        PAYLOAD_HEADER_BYTES
            + prepared.preview_pixels.len()
            + prepared.overview_pixels.len()
            + hist_bytes,
    );

    payload.extend_from_slice(&prepared.preview_width.to_le_bytes());
    payload.extend_from_slice(&prepared.preview_height.to_le_bytes());
    payload.extend_from_slice(&prepared.full_width.to_le_bytes());
    payload.extend_from_slice(&prepared.full_height.to_le_bytes());
    payload.extend_from_slice(&prepared.overview_width.to_le_bytes());
    payload.extend_from_slice(&prepared.overview_height.to_le_bytes());
    payload.extend_from_slice(&prepared.preview_pixels);
    payload.extend_from_slice(&prepared.overview_pixels);

    let h = &prepared.histograms;
    for &v in h.r.iter().chain(h.g.iter()).chain(h.b.iter()) {
        payload.extend_from_slice(&v.to_le_bytes());
    }

    payload
}

/// Expected payload length for a preview and overview of the given dimensions.
pub fn payload_len_for(
    preview_width: u32,
    preview_height: u32,
    overview_width: u32,
    overview_height: u32,
) -> usize {
    let pixels = |w: u32, h: u32| (w as usize) * (h as usize) * LINEAR_BYTES_PER_PIXEL;

    PAYLOAD_HEADER_BYTES
        + pixels(preview_width, preview_height)
        + pixels(overview_width, overview_height)
        + 3 * HIST_BINS * 4
}

// ── Tauri command ─────────────────────────────────────────────────────────────

/// Error returned when the user dismisses the file picker.
///
/// Dismissal happens before the loaded image is released, so it is the one
/// failure that leaves the current image intact — `openImage.ts` matches on this
/// exact string to tell it apart from an open that got far enough to purge.
pub const OPEN_CANCELLED: &str = "open:cancelled";

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
        .ok_or(OPEN_CANCELLED)?
        .into_path()
        .map_err(|e| e.to_string())?;

    // Release the previous image before decoding the next one. Held any longer
    // and two full-resolution buffers are resident at once, which for large
    // files roughly doubles the peak. The dialog has already returned a path, so
    // a cancelled open never reaches here and leaves the current image alone.
    {
        let mut guard = state.lock().unwrap();
        *guard = None;
    }

    let overall_start = Instant::now();

    let img = image::open(&path).map_err(|e| format!("Failed to open image: {e}"))?;

    let (full_width, full_height) = (img.width(), img.height());

    // Sources whose format declares no alpha are kept three-wide. `into_rgb16` /
    // `into_rgba16` consume the DynamicImage, so the decoded source buffer is
    // released as soon as the conversion is done rather than idling.
    let (samples, channels) = if img.color().has_alpha() {
        (img.into_rgba16().into_raw(), 4)
    } else {
        (img.into_rgb16().into_raw(), 3)
    };

    let prepared = prepare_image(&samples, full_width, full_height, channels);

    // Store the original full-resolution image for export. This happens after
    // `prepare_image` so the buffer can be moved into the state rather than
    // cloned — preparation only needed to borrow it.
    {
        let mut guard = state.lock().unwrap();
        *guard = Some(OriginalImage {
            pixels_u16: Arc::new(samples),
            width: full_width,
            height: full_height,
            channels,
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
