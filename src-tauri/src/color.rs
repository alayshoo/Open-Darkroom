// src-tauri/src/color.rs
//
// Colour-space conversion shared by the preview path (image_opening) and the
// export path (export_rendering). The preview uploads `rgba16float` and the
// export `rgba32float`, so the tables differ in width but not in curve —
// keeping one definition of that curve is what guarantees the on-screen preview
// and the exported file agree.

use half::f16;
use rayon::prelude::*;

/// Entries in the sRGB → linear lookup table (the full u16 domain).
pub const LUT_SIZE: usize = 65536;

/// Bytes per pixel once linearised: 4 channels × f16.
pub const LINEAR_BYTES_PER_PIXEL: usize = 8;

/// Bytes per pixel once linearised at full width: 4 channels × f32.
pub const LINEAR_F32_BYTES_PER_PIXEL: usize = 16;

/// The sRGB electro-optical transfer function, over the unit interval.
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055_f32).powf(2.4)
    }
}

/// sRGB u16 → linear f16 lookup table.
///
/// Built once per image load and shared across the rayon workers, so the
/// per-pixel cost is a single table lookup rather than a `powf`.
///
/// This is the preview's table. f16 resolves ~13.4 of the 16 input bits, which
/// is far more than an 8-bit display can show and half the VRAM of f32; the
/// export uses [`build_srgb_to_linear_lut_f32`] instead, where the missing bits
/// do reach the file.
pub fn build_srgb_to_linear_lut_u16() -> Vec<f16> {
    (0..LUT_SIZE)
        .map(|i| f16::from_f32(srgb_to_linear(i as f32 / (LUT_SIZE - 1) as f32)))
        .collect()
}

/// sRGB u16 → linear f32 lookup table.
///
/// The export's table. Every one of the 65536 input codes maps to a distinct
/// value, so nothing is lost before the shader sees it.
pub fn build_srgb_to_linear_lut_f32() -> Vec<f32> {
    (0..LUT_SIZE)
        .map(|i| srgb_to_linear(i as f32 / (LUT_SIZE - 1) as f32))
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
            let linear = srgb_to_linear(i as f32 / (LUT_SIZE - 1) as f32);
            (linear * (LUT_SIZE - 1) as f32).round() as u16
        })
        .collect()
}

/// Samples per pixel a source buffer may carry: RGB, or RGB with alpha.
///
/// Images whose format declares no alpha channel are decoded and stored as three
/// channels, which is a quarter less memory held for the life of the session.
/// Everything uploaded to the GPU is still RGBA — WebGPU offers no
/// three-channel float texture format — so the alpha is synthesised at the
/// point of upload rather than carried through the buffer.
fn assert_channels(channels: usize) {
    debug_assert!(
        channels == 3 || channels == 4,
        "channels must be 3 (RGB) or 4 (RGBA), got {channels}"
    );
}

/// Gamma-decode interleaved u16 sRGB pixels into interleaved u16 *linear*
/// pixels, ready to be resampled. The channel count is preserved.
///
/// Alpha carries no gamma encoding, so where present it is passed through
/// untouched rather than sent through `lut`.
pub fn srgb_to_linear_u16(src: &[u16], channels: usize, lut: &[u16]) -> Vec<u16> {
    assert_channels(channels);
    debug_assert_eq!(src.len() % channels, 0, "input must be whole pixels");
    debug_assert_eq!(lut.len(), LUT_SIZE, "lut must cover the full u16 domain");

    let mut out = vec![0u16; src.len()];

    out.par_chunks_exact_mut(channels)
        .enumerate()
        .for_each(|(i, chunk)| {
            let s = i * channels;
            chunk[0] = lut[src[s] as usize];
            chunk[1] = lut[src[s + 1] as usize];
            chunk[2] = lut[src[s + 2] as usize];
            if channels == 4 {
                chunk[3] = src[s + 3];
            }
        });

    out
}

/// Pack interleaved u16 *linear* pixels into the little-endian RGBA f16 bytes an
/// `rgba16float` texture expects.
///
/// The counterpart to [`srgb_to_linear_u16`]: that decodes, this uploads. Every
/// channel is already linear here, alpha included, so all of them scale the same
/// way. A three-channel source is uploaded fully opaque.
pub fn linear_u16_to_f16_bytes(src: &[u16], channels: usize) -> Vec<u8> {
    assert_channels(channels);
    debug_assert_eq!(src.len() % channels, 0, "input must be whole pixels");

    let pixel_count = src.len() / channels;
    let mut out = vec![0u8; pixel_count * LINEAR_BYTES_PER_PIXEL];

    out.par_chunks_exact_mut(LINEAR_BYTES_PER_PIXEL)
        .enumerate()
        .for_each(|(i, chunk)| {
            let s = i * channels;
            for c in 0..3 {
                let v = f16::from_f32(src[s + c] as f32 / (LUT_SIZE - 1) as f32);
                chunk[c * 2..c * 2 + 2].copy_from_slice(&v.to_le_bytes());
            }
            let a = if channels == 4 {
                f16::from_f32(src[s + 3] as f32 / (LUT_SIZE - 1) as f32)
            } else {
                f16::ONE
            };
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

    out
}

/// Convert interleaved u16 sRGB pixels into little-endian RGBA f16 linear bytes,
/// ready to upload as an `rgba16float` texture.
///
/// RGB is gamma-decoded through `lut`; alpha, where the source carries it, is
/// scaled linearly, since alpha carries no gamma encoding. A three-channel
/// source is uploaded fully opaque.
///
/// `src.len()` is expected to be a multiple of `channels`; any trailing partial
/// pixel is ignored.
pub fn linearize_u16(src: &[u16], channels: usize, lut: &[f16]) -> Vec<u8> {
    assert_channels(channels);
    debug_assert_eq!(src.len() % channels, 0, "input must be whole pixels");
    debug_assert_eq!(lut.len(), LUT_SIZE, "lut must cover the full u16 domain");

    let pixel_count = src.len() / channels;
    let mut out = vec![0u8; pixel_count * LINEAR_BYTES_PER_PIXEL];

    out.par_chunks_exact_mut(LINEAR_BYTES_PER_PIXEL)
        .enumerate()
        .for_each(|(i, chunk)| {
            let s = i * channels;
            let r = lut[src[s] as usize];
            let g = lut[src[s + 1] as usize];
            let b = lut[src[s + 2] as usize];
            let a = if channels == 4 {
                f16::from_f32(src[s + 3] as f32 / (LUT_SIZE - 1) as f32)
            } else {
                f16::ONE
            };
            chunk[0..2].copy_from_slice(&r.to_le_bytes());
            chunk[2..4].copy_from_slice(&g.to_le_bytes());
            chunk[4..6].copy_from_slice(&b.to_le_bytes());
            chunk[6..8].copy_from_slice(&a.to_le_bytes());
        });

    out
}

/// Convert interleaved u16 sRGB pixels into little-endian RGBA f32 linear bytes,
/// ready to upload as an `rgba32float` texture.
///
/// The export's counterpart to [`linearize_u16`], and identical to it but for
/// the width of the destination. f16 spacing in the top linear octave is 4.9e-4
/// while one 16-bit sRGB code near white is 3.5e-5 apart, so an f16 upload
/// collapses runs of about seventeen adjacent input codes onto one value —
/// visible as banding once the render is written back out at 16 bits.
pub fn linearize_u16_f32(src: &[u16], channels: usize, lut: &[f32]) -> Vec<u8> {
    assert_channels(channels);
    debug_assert_eq!(src.len() % channels, 0, "input must be whole pixels");
    debug_assert_eq!(lut.len(), LUT_SIZE, "lut must cover the full u16 domain");

    let pixel_count = src.len() / channels;
    let mut out = vec![0u8; pixel_count * LINEAR_F32_BYTES_PER_PIXEL];

    out.par_chunks_exact_mut(LINEAR_F32_BYTES_PER_PIXEL)
        .enumerate()
        .for_each(|(i, chunk)| {
            let s = i * channels;
            let a = if channels == 4 {
                src[s + 3] as f32 / (LUT_SIZE - 1) as f32
            } else {
                1.0
            };
            let rgba = [
                lut[src[s] as usize],
                lut[src[s + 1] as usize],
                lut[src[s + 2] as usize],
                a,
            ];
            for (c, v) in rgba.iter().enumerate() {
                chunk[c * 4..c * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        });

    out
}
