// src-tauri/src/export_size.rs
//
// The size an export is written at. The render always runs at full resolution
// and is resampled afterwards, in linear light, so a downscale keeps the
// brightness of fine detail. Mirrored by targetSize() in src/lib/types/exportSettings.ts.

use fast_image_resize as fir;
use fast_image_resize::images::{Image, ImageRef};
use rayon::prelude::*;

use crate::output_space::{Transfer, ADOBE_GAMMA, BT2020_ALPHA, BT2020_BETA};

/// Source rows a 16-bit resample converts to f32 at once.
const SOURCE_BAND_ROWS: u32 = 512;

#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    #[default]
    None,
    LongEdge,
    ShortEdge,
    Width,
    Height,
    Percent,
}

#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputSize {
    pub mode: ResizeMode,
    /// The edge length for every mode but `Percent`.
    pub pixels: u32,
    pub percent: f64,
}

/// The dimensions a `width` × `height` render is written at. An export is
/// never enlarged.
pub fn target_size(size: &OutputSize, width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width as f64, height as f64);
    let pixels = f64::from(size.pixels.max(1));
    let scale = match size.mode {
        ResizeMode::None => 1.0,
        ResizeMode::LongEdge => pixels / w.max(h),
        ResizeMode::ShortEdge => pixels / w.min(h),
        ResizeMode::Width => pixels / w,
        ResizeMode::Height => pixels / h,
        ResizeMode::Percent if size.percent.is_finite() && size.percent > 0.0 => size.percent / 100.0,
        ResizeMode::Percent => 1.0,
    };
    if scale >= 1.0 {
        return (width, height);
    }
    let side = |v: f64| ((v * scale).round() as u32).clamp(1, u32::MAX);
    (side(w), side(h))
}

/// Resample interleaved RGB, 8-bit or 16-bit little-endian, from `from` to `to`.
pub fn resize_rgb(
    rgb: Vec<u8>,
    from: (u32, u32),
    to: (u32, u32),
    bit_depth: u8,
    transfer: Transfer,
) -> Result<Vec<u8>, String> {
    if from == to {
        return Ok(rgb);
    }
    let bytes_per_sample = if bit_depth > 8 { 2 } else { 1 };
    let expected = from.0 as usize * from.1 as usize * 3 * bytes_per_sample;
    if rgb.len() != expected {
        return Err(format!("Resize input is {} bytes, expected {expected}", rgb.len()));
    }
    let options = fir::ResizeOptions::new()
        .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Lanczos3));
    let mut resizer = fir::Resizer::new();
    let err = |e: &dyn std::fmt::Display| format!("Resize failed: {e}");

    if bytes_per_sample == 1 {
        // Linear u16 holds every 8-bit step apart, and halves the memory f32 would take.
        let mapper = fir::PixelComponentMapper::new(
            move |v| to_linear(transfer, v),
            move |v| from_linear(transfer, v),
        );
        let source = ImageRef::new(from.0, from.1, &rgb, fir::PixelType::U8x3).map_err(|e| err(&e))?;
        let mut linear = Image::new(from.0, from.1, fir::PixelType::U16x3);
        mapper.forward_map(&source, &mut linear).map_err(|e| err(&e))?;
        drop(rgb);

        let mut resized = Image::new(to.0, to.1, fir::PixelType::U16x3);
        resizer.resize(&linear, &mut resized, &options).map_err(|e| err(&e))?;
        drop(linear);

        let mut out = Image::new(to.0, to.1, fir::PixelType::U8x3);
        mapper.backward_map(&resized, &mut out).map_err(|e| err(&e))?;
        return Ok(out.into_vec());
    }

    // A 16-bit file's deepest shadows need more than linear u16 can tell apart,
    // so it is resampled as f32, a band of rows at a time to keep that copy small.
    let table: Vec<f32> = (0..=u16::MAX)
        .map(|code| to_linear(transfer, f32::from(code) / 65535.0))
        .collect();
    let row_bytes = from.0 as usize * 3 * 2;
    let scale = f64::from(from.1) / f64::from(to.1);
    // How far past its band the filter reaches into the source.
    let margin = (3.0 * scale.max(1.0)).ceil() as u32 + 1;
    let band_rows = ((SOURCE_BAND_ROWS as f64 / scale) as u32).max(1);

    let mut out: Vec<u8> = Vec::with_capacity(to.0 as usize * to.1 as usize * 3 * 2);
    for out_top in (0..to.1).step_by(band_rows as usize) {
        let out_rows = band_rows.min(to.1 - out_top);
        let in_top = f64::from(out_top) * scale;
        let in_bottom = f64::from(out_top + out_rows) * scale;
        let first = (in_top.floor() as u32).saturating_sub(margin);
        let last = (in_bottom.ceil() as u32 + margin).min(from.1);

        let band: Vec<f32> = rgb[first as usize * row_bytes..last as usize * row_bytes]
            .par_chunks_exact(2)
            .map(|b| table[usize::from(u16::from_le_bytes([b[0], b[1]]))])
            .collect();
        let source = ImageRef::new(from.0, last - first, bytemuck::cast_slice(&band), fir::PixelType::F32x3)
            .map_err(|e| err(&e))?;
        let mut resized = Image::new(to.0, out_rows, fir::PixelType::F32x3);
        let cropped = options.crop(0.0, in_top - f64::from(first), f64::from(from.0), in_bottom - in_top);
        resizer.resize(&source, &mut resized, &cropped).map_err(|e| err(&e))?;

        out.par_extend(resized.buffer().par_chunks_exact(4).flat_map_iter(|b| {
            let v = f32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
            let code = (from_linear(transfer, v.clamp(0.0, 1.0)) * 65535.0).round() as u16;
            code.to_le_bytes()
        }));
    }
    Ok(out)
}

fn to_linear(transfer: Transfer, v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    match transfer {
        Transfer::Srgb if v <= 0.04045 => v / 12.92,
        Transfer::Srgb => ((v + 0.055) / 1.055).powf(2.4),
        Transfer::Adobe => v.powf(ADOBE_GAMMA as f32),
        Transfer::Bt2020 => {
            let (alpha, beta) = (BT2020_ALPHA as f32, BT2020_BETA as f32);
            if v < 4.5 * beta { v / 4.5 } else { ((v + alpha - 1.0) / alpha).powf(1.0 / 0.45) }
        }
    }
}

fn from_linear(transfer: Transfer, v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    match transfer {
        Transfer::Srgb if v <= 0.003_130_8 => v * 12.92,
        Transfer::Srgb => 1.055 * v.powf(1.0 / 2.4) - 0.055,
        Transfer::Adobe => v.powf(1.0 / ADOBE_GAMMA as f32),
        Transfer::Bt2020 => {
            let (alpha, beta) = (BT2020_ALPHA as f32, BT2020_BETA as f32);
            if v < beta { 4.5 * v } else { alpha * v.powf(0.45) - (alpha - 1.0) }
        }
    }
}
