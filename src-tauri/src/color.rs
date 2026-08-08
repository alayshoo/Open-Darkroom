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
