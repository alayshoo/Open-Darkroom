// src-tauri/src/full_resolution.rs
//
// The whole image as the preview keeps it on the GPU: linear RGBA f16, served a
// band of rows at a time so no single transfer has to hold all of it. An image
// larger than the GPU's texture limit is served resized to fit.

use half::f16;
use std::sync::{Arc, Mutex, OnceLock};

use tauri::ipc::Response;

use crate::color::{linear_u16_to_f16_bytes, linearize_u16, to_linear_u16, LUT_SIZE};
use crate::icc::{Baker, SourceSpace};
use crate::image_opening::{downscale_to_max_edge, ImageState};

/// Bytes preceding the pixels of a band: the size of the image being served,
/// then the band's first row and row count, u32 LE.
pub const ROWS_HEADER_BYTES: usize = 16;

/// Rows baked and linearised at a time while fitting, so the baked path never
/// holds a second full-size copy of the source.
const FIT_CHUNK_ROWS: usize = 256;

/// What serving one image needs beyond the source itself, built on first use.
#[derive(Default)]
pub struct FullResolution {
    luts: OnceLock<[Vec<f16>; 3]>,
    baker: OnceLock<Result<Option<Baker>, String>>,
    /// Only held until its last band has been served.
    fitted: Mutex<Option<Arc<Fitted>>>,
}

struct Fitted {
    max_edge: u32,
    width: u32,
    linear: Vec<u16>,
}

/// A band of rows of an image, with the size of the image it was cut from.
pub struct Rows {
    pub width: u32,
    pub height: u32,
    pub y: u32,
    pub rows: u32,
    pub pixels: Vec<u8>,
}

/// The size an image is served at for a GPU whose textures reach `max_edge`.
pub fn served_size(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    if width.max(height) <= max_edge {
        return (width, height);
    }
    let scale = max_edge as f32 / width.max(height) as f32;
    (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

impl FullResolution {
    /// Rows `y..y + rows` of the image served for `max_edge`, cut short at the
    /// bottom of the image.
    #[allow(clippy::too_many_arguments)]
    pub fn rows(
        &self,
        src: &[u16],
        width: u32,
        height: u32,
        channels: usize,
        space: &SourceSpace,
        max_edge: u32,
        y: u32,
        rows: u32,
    ) -> Result<Rows, String> {
        if max_edge == 0 {
            return Err("max_edge must be non-zero".into());
        }
        let (served_w, served_h) = served_size(width, height, max_edge);
        if y >= served_h {
            return Err(format!(
                "Row {y} is past the bottom of a {served_h}-row image"
            ));
        }
        let rows = rows.min(served_h - y);
        let band = |w: u32| {
            (y as usize * w as usize * channels)..((y + rows) as usize * w as usize * channels)
        };

        let baker = self
            .baker
            .get_or_init(|| space.baker())
            .as_ref()
            .map_err(Clone::clone)?
            .as_ref();

        let pixels = if (served_w, served_h) == (width, height) {
            let cut = &src[band(width)];
            let baked = baker.map(|b| b.apply(cut, channels)).transpose()?;
            let luts = self.luts.get_or_init(|| space.luts(f16::from_f32));
            linearize_u16(baked.as_deref().unwrap_or(cut), channels, luts)
        } else {
            let fitted = {
                let mut held = self.fitted.lock().unwrap();
                match held.as_ref().filter(|f| f.max_edge == max_edge) {
                    Some(f) => Arc::clone(f),
                    None => {
                        let f = Arc::new(fit(src, width, height, channels, space, baker, max_edge)?);
                        *held = Some(Arc::clone(&f));
                        f
                    }
                }
            };
            let pixels = linear_u16_to_f16_bytes(&fitted.linear[band(fitted.width)], channels);
            if y + rows == served_h {
                *self.fitted.lock().unwrap() = None;
            }
            pixels
        };

        Ok(Rows {
            width: served_w,
            height: served_h,
            y,
            rows,
            pixels,
        })
    }
}

/// The whole image resampled in linear light to fit `max_edge`, as the preview
/// is.
fn fit(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    space: &SourceSpace,
    baker: Option<&Baker>,
    max_edge: u32,
) -> Result<Fitted, String> {
    let luts = space.luts(|v| (v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32).round() as u16);
    let linear = match baker {
        None => to_linear_u16(src, channels, &luts),
        Some(baker) => {
            let mut linear = Vec::with_capacity(src.len());
            for chunk in src.chunks(FIT_CHUNK_ROWS * width as usize * channels) {
                linear.extend(to_linear_u16(&baker.apply(chunk, channels)?, channels, &luts));
            }
            linear
        }
    };
    let (linear, width, _) = downscale_to_max_edge(&linear, width, height, channels, max_edge);
    Ok(Fitted {
        max_edge,
        width,
        linear,
    })
}

/// Serialise a band into the layout `fullResolution.ts` parses: the header
/// fields as u32 LE, then w × r RGBA f16 LE pixels.
pub fn build_rows_payload(rows: &Rows) -> Vec<u8> {
    let mut payload = Vec::with_capacity(ROWS_HEADER_BYTES + rows.pixels.len());
    for v in [rows.width, rows.height, rows.y, rows.rows] {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    payload.extend_from_slice(&rows.pixels);
    payload
}

// ── Tauri command ─────────────────────────────────────────────────────────────

/// A band of rows of the open image, for a GPU whose textures reach `max_edge`.
#[tauri::command]
pub(crate) async fn full_resolution_rows(
    state: tauri::State<'_, ImageState>,
    image_id: u32,
    max_edge: u32,
    y: u32,
    rows: u32,
) -> Result<Response, String> {
    let (pixels, width, height, channels, space, full) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        if img.id != image_id {
            return Err("The image has been replaced since this band was asked for".into());
        }
        (
            Arc::clone(&img.pixels_u16),
            img.width,
            img.height,
            img.channels,
            img.space.clone(),
            Arc::clone(&img.full_resolution),
        )
    };

    // Off the async runtime: converting a band, and above all resizing the
    // whole image the first time a fitted one is asked for, would stall it.
    let payload = tauri::async_runtime::spawn_blocking(move || {
        full.rows(&pixels, width, height, channels, &space, max_edge, y, rows)
            .map(|r| build_rows_payload(&r))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(Response::new(payload))
}
