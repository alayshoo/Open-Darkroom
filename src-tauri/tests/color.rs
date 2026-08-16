// src-tauri/tests/color.rs
//
// Layer 1: the sRGB → linear conversion shared by the preview and export paths.
// Everything downstream is built on this, so an error here is invisible but
// systematic.

mod common;

use common::{linear_to_srgb, srgb_to_linear, u8_to_u16, EXTREME_VALUES};

use open_darkroom_lib::color::{
    build_srgb_to_linear_lut_u16, linearize_u16, LINEAR_BYTES_PER_PIXEL, LUT_SIZE,
};

#[test]
fn lut_spans_the_domain_and_is_monotonic() {
    let lut = build_srgb_to_linear_lut_u16();

    assert_eq!(lut.len(), LUT_SIZE);
    assert_eq!(lut[0].to_f32(), 0.0, "black must stay black");
    assert!(
        (lut[LUT_SIZE - 1].to_f32() - 1.0).abs() < 1e-3,
        "white must map to 1.0, got {}",
        lut[LUT_SIZE - 1].to_f32()
    );

    for i in 1..LUT_SIZE {
        assert!(
            lut[i].to_f32() >= lut[i - 1].to_f32(),
            "LUT must be non-decreasing, broke at {i}"
        );
    }
}

#[test]
fn lut_tracks_the_reference_curve() {
    let lut = build_srgb_to_linear_lut_u16();

    // f16 carries ~11 bits of mantissa, so compare in relative terms away from
    // zero and in absolute terms near it.
    for i in (0..LUT_SIZE).step_by(97) {
        let expected = srgb_to_linear(i as f64 / (LUT_SIZE - 1) as f64);
        let actual = lut[i].to_f32() as f64;
        let tolerance = (expected * 1e-3).max(1e-7);
        assert!(
            (actual - expected).abs() <= tolerance,
            "index {i}: expected {expected:.8}, got {actual:.8}"
        );
    }
}

/// The pipeline's precision floor: how much error f16 linear storage costs once
/// the value is encoded back to 8-bit sRGB for display.
#[test]
fn f16_storage_survives_a_round_trip_to_8_bit() {
    let lut = build_srgb_to_linear_lut_u16();

    let mut worst = 0.0f64;
    for v in 0u16..=255 {
        let stored = lut[u8_to_u16(v as u8) as usize].to_f32() as f64;
        let back = linear_to_srgb(stored) * 255.0;
        worst = worst.max((back - v as f64).abs());
    }

    assert!(
        worst < 0.5,
        "f16 linear storage costs {worst:.4} of 255 — more than half a code value"
    );
}

#[test]
fn linearize_produces_eight_bytes_per_pixel() {
    let lut = build_srgb_to_linear_lut_u16();
    let input = vec![0u16; 4 * 7];

    let out = linearize_u16(&input, 4, &lut);

    assert_eq!(out.len(), 7 * LINEAR_BYTES_PER_PIXEL);
}

#[test]
fn linearize_maps_each_channel_through_the_lut() {
    let lut = build_srgb_to_linear_lut_u16();

    // One pixel per extreme value, with a distinct value in every channel.
    let mut input = Vec::new();
    for (i, &v) in EXTREME_VALUES.iter().enumerate() {
        let next = EXTREME_VALUES[(i + 1) % EXTREME_VALUES.len()];
        let other = EXTREME_VALUES[(i + 2) % EXTREME_VALUES.len()];
        input.extend_from_slice(&[v, next, other, u16::MAX]);
    }

    let out = linearize_u16(&input, 4, &lut);

    for (px, chunk) in out.chunks_exact(LINEAR_BYTES_PER_PIXEL).enumerate() {
        for c in 0..3 {
            let got = half::f16::from_le_bytes([chunk[c * 2], chunk[c * 2 + 1]]);
            let expected = lut[input[px * 4 + c] as usize];
            assert_eq!(
                got, expected,
                "pixel {px} channel {c} did not come from the LUT"
            );
        }

        // Alpha is scaled linearly, never gamma-decoded.
        let alpha = half::f16::from_le_bytes([chunk[6], chunk[7]]);
        assert!(
            (alpha.to_f32() - 1.0).abs() < 1e-3,
            "opaque alpha must stay 1.0, got {}",
            alpha.to_f32()
        );
    }
}

#[test]
fn linearize_is_order_independent() {
    // The conversion is parallelised with rayon; a data race or a bad index
    // would show up as pixels influenced by their neighbours.
    let lut = build_srgb_to_linear_lut_u16();

    let single = linearize_u16(&[300, 20_000, 65_535, u16::MAX], 4, &lut);

    let mut padded = vec![0u16; 4 * 500];
    padded[4 * 250..4 * 251].copy_from_slice(&[300, 20_000, 65_535, u16::MAX]);
    let batch = linearize_u16(&padded, 4, &lut);

    let at = 250 * LINEAR_BYTES_PER_PIXEL;
    assert_eq!(
        &batch[at..at + LINEAR_BYTES_PER_PIXEL],
        &single[..],
        "a pixel's conversion must not depend on its neighbours"
    );
}

// ── Three-channel sources ─────────────────────────────────────────────────────

/// A source whose format declares no alpha is stored three-wide, but the texture
/// it is uploaded to is `rgba16float` either way — so the conversion has to widen
/// while it linearises, and the result must be indistinguishable from the same
/// pixels carried with an opaque alpha.
#[test]
fn linearize_widens_three_channels_to_an_opaque_rgba() {
    let lut = build_srgb_to_linear_lut_u16();

    let mut rgb = Vec::new();
    let mut rgba = Vec::new();
    for (i, &v) in EXTREME_VALUES.iter().enumerate() {
        let next = EXTREME_VALUES[(i + 1) % EXTREME_VALUES.len()];
        let other = EXTREME_VALUES[(i + 2) % EXTREME_VALUES.len()];
        rgb.extend_from_slice(&[v, next, other]);
        rgba.extend_from_slice(&[v, next, other, u16::MAX]);
    }

    let from_rgb = linearize_u16(&rgb, 3, &lut);
    let from_rgba = linearize_u16(&rgba, 4, &lut);

    assert_eq!(
        from_rgb.len(),
        EXTREME_VALUES.len() * LINEAR_BYTES_PER_PIXEL,
        "the upload is RGBA whatever the source carried"
    );
    assert_eq!(
        from_rgb, from_rgba,
        "an opaque RGBA source and its RGB equivalent must upload identically"
    );
}

#[test]
fn a_three_channel_upload_is_fully_opaque() {
    let lut = build_srgb_to_linear_lut_u16();
    let out = linearize_u16(&[0, 32_768, 65_535], 3, &lut);

    let alpha = half::f16::from_le_bytes([out[6], out[7]]);
    assert_eq!(alpha.to_f32(), 1.0, "synthesised alpha must be exactly 1.0");
}
