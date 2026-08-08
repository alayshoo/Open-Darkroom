// src-tauri/src/color.rs
//
// Colour-space conversion shared by the preview path (image_opening) and the
// export path (export_rendering). Both upload `rgba16float` textures, so both
// need the same u16 sRGB → f16 linear conversion — keeping a single copy is
// what guarantees the on-screen preview and the exported file agree.

use half::f16;
use rayon::prelude::*;

/// Entries in the sRGB → linear lookup table (the full u16 domain).
pub const LUT_SIZE: usize = 65536;

/// Bytes per pixel once linearised: 4 channels × f16.
pub const LINEAR_BYTES_PER_PIXEL: usize = 8;

/// sRGB u16 → linear f16 lookup table.
///
/// Built once per image load / export and shared across the rayon workers, so
/// the per-pixel cost is a single table lookup rather than a `powf`.
pub fn build_srgb_to_linear_lut_u16() -> Vec<f16> {
    (0..LUT_SIZE)
        .map(|i| {
            let s = i as f32 / (LUT_SIZE - 1) as f32;
            let linear = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055_f32).powf(2.4)
            };
            f16::from_f32(linear)
        })
        .collect()
}

/// sRGB u16 → linear u16 lookup table.
///
/// The preview is resampled before it reaches the GPU, and resampling has to
/// happen in linear light — see `prepare_image` for what averaging the encoding
/// instead costs. `fast_image_resize` has no f16 pixel type, so linear u16 is
/// the widest format both it and the RGBA16 buffer layout speak.
///
/// The quantisation that costs is confined to the preview and sits below
/// anything visible. One linear u16 step is 1.5e-5, while the darkest tone an
/// 8-bit source can carry — sRGB 1/255 — is linear 3.0e-4, twenty steps up. A
/// true 16-bit source loses its bottom six code values of 65536. The export
/// path never resamples and never reads this table.
pub fn build_resample_lut() -> Vec<u16> {
    (0..LUT_SIZE)
        .map(|i| {
            let s = i as f32 / (LUT_SIZE - 1) as f32;
            let linear = if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055_f32).powf(2.4)
            };
            (linear * (LUT_SIZE - 1) as f32).round() as u16
        })
        .collect()
}

/// Gamma-decode interleaved RGBA u16 sRGB pixels into interleaved RGBA u16
/// *linear* pixels, ready to be resampled.
///
/// Alpha carries no gamma encoding, so it is passed through untouched rather
/// than sent through `lut`.
pub fn srgb_to_linear_rgba_u16(rgba_u16: &[u16], lut: &[u16]) -> Vec<u16> {
    debug_assert_eq!(rgba_u16.len() % 4, 0, "input must be whole RGBA pixels");
    debug_assert_eq!(lut.len(), LUT_SIZE, "lut must cover the full u16 domain");

    let mut out = vec![0u16; rgba_u16.len()];

    out.par_chunks_exact_mut(4)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            chunk[0] = lut[rgba_u16[src] as usize];
            chunk[1] = lut[rgba_u16[src + 1] as usize];
            chunk[2] = lut[rgba_u16[src + 2] as usize];
            chunk[3] = rgba_u16[src + 3];
        });

    out
}

/// Pack interleaved RGBA u16 *linear* pixels into the little-endian f16 bytes an
/// `rgba16float` texture expects.
///
/// The counterpart to [`srgb_to_linear_rgba_u16`]: that decodes, this uploads.
/// Every channel is already linear here, alpha included, so all four scale the
/// same way.
pub fn linear_u16_to_f16_bytes(rgba_u16: &[u16]) -> Vec<u8> {
    debug_assert_eq!(rgba_u16.len() % 4, 0, "input must be whole RGBA pixels");

    let pixel_count = rgba_u16.len() / 4;
    let mut out = vec![0u8; pixel_count * LINEAR_BYTES_PER_PIXEL];

    out.par_chunks_exact_mut(LINEAR_BYTES_PER_PIXEL)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            for c in 0..4 {
                let v = f16::from_f32(rgba_u16[src + c] as f32 / (LUT_SIZE - 1) as f32);
                chunk[c * 2..c * 2 + 2].copy_from_slice(&v.to_le_bytes());
            }
        });

    out
}

/// Convert interleaved RGBA u16 sRGB pixels into little-endian RGBA f16 linear
/// bytes, ready to upload as an `rgba16float` texture.
///
/// RGB is gamma-decoded through `lut`; alpha is scaled linearly, since alpha
/// carries no gamma encoding.
///
/// `rgba_u16.len()` is expected to be a multiple of 4; any trailing partial
/// pixel is ignored.
pub fn linearize_rgba_u16(rgba_u16: &[u16], lut: &[f16]) -> Vec<u8> {
    debug_assert_eq!(rgba_u16.len() % 4, 0, "input must be whole RGBA pixels");
    debug_assert_eq!(lut.len(), LUT_SIZE, "lut must cover the full u16 domain");

    let pixel_count = rgba_u16.len() / 4;
    let mut out = vec![0u8; pixel_count * LINEAR_BYTES_PER_PIXEL];

    out.par_chunks_exact_mut(LINEAR_BYTES_PER_PIXEL)
        .enumerate()
        .for_each(|(i, chunk)| {
            let src = i * 4;
            let r = lut[rgba_u16[src] as usize];
            let g = lut[rgba_u16[src + 1] as usize];
            let b = lut[rgba_u16[src + 2] as usize];
            let a = f16::from_f32(rgba_u16[src + 3] as f32 / (LUT_SIZE - 1) as f32);
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

    out
}
