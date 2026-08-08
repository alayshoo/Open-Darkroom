// src-tauri/tests/image_prep.rs
//
// Layer 1: everything between "the file is decoded" and "the frontend has a
// texture" — downscaling, histograms, and the payload wire format.

mod common;

use common::{gray_ramp, noise, rgb_ramp, solid, u8_to_u16};

use open_darkroom_lib::color::{build_srgb_to_linear_lut_u16, linearize_rgba_u16, LINEAR_BYTES_PER_PIXEL};
use open_darkroom_lib::image_opening::{
    build_payload, compute_histograms, downscale_to_max_edge, payload_len_for, prepare_image,
    HIST_BINS, PAYLOAD_HEADER_BYTES, PREVIEW_MAX_EDGE,
};

// ── Downscaling ───────────────────────────────────────────────────────────────

#[test]
fn downscale_preserves_aspect_ratio() {
    let cases = [
        // (source, expected preview)
        ((4000, 2000), (2048, 1024)),
        ((2000, 4000), (1024, 2048)),
        ((4096, 4096), (2048, 2048)),
        ((3000, 1000), (2048, 683)),
    ];

    for ((w, h), expected) in cases {
        let img = solid(w, h, [0, 0, 0, u16::MAX]);
        let small = downscale_to_max_edge(&img, PREVIEW_MAX_EDGE);
        assert_eq!(small.dimensions(), expected, "downscaling {w}×{h}");

        let src_aspect = w as f64 / h as f64;
        let dst_aspect = expected.0 as f64 / expected.1 as f64;
        assert!(
            (src_aspect - dst_aspect).abs() / src_aspect < 0.002,
            "{w}×{h}: aspect drifted from {src_aspect:.4} to {dst_aspect:.4}"
        );
    }
}

#[test]
fn downscale_leaves_small_images_untouched() {
    let img = rgb_ramp(64, 32);
    let same = downscale_to_max_edge(&img, PREVIEW_MAX_EDGE);

    assert_eq!(same.dimensions(), (64, 32));
    assert_eq!(same.as_raw(), img.as_raw(), "no resampling should occur");

    // Exactly at the limit is still untouched.
    let edge = solid(PREVIEW_MAX_EDGE, 100, [7, 7, 7, u16::MAX]);
    assert_eq!(
        downscale_to_max_edge(&edge, PREVIEW_MAX_EDGE).dimensions(),
        (PREVIEW_MAX_EDGE, 100)
    );
}

#[test]
fn downscale_preserves_a_flat_colour_exactly() {
    // Resampling a constant image must be a no-op on the values. This is what
    // catches a botched u16 byte re-interpretation in the resize round trip.
    let img = solid(4000, 2000, [1234, 5678, 9012, u16::MAX]);
    let small = downscale_to_max_edge(&img, PREVIEW_MAX_EDGE);

    for px in small.as_raw().chunks_exact(4) {
        assert_eq!([px[0], px[1], px[2]], [1234, 5678, 9012]);
    }
}

#[test]
fn downscale_keeps_channels_distinct() {
    // A channel swap during the byte round trip would survive the flat-colour
    // test, so check a gradient where the channels disagree.
    let img = rgb_ramp(3000, 100);
    let small = downscale_to_max_edge(&img, PREVIEW_MAX_EDGE);

    let (w, _) = small.dimensions();
    let last = small.get_pixel(w - 1, 0).0;
    let first = small.get_pixel(0, 0).0;

    assert!(last[0] > first[0], "red ramps up");
    assert!(last[1] < first[1], "green ramps down");
}

// ── Histograms ────────────────────────────────────────────────────────────────

#[test]
fn histogram_counts_every_pixel_once() {
    let img = noise(200, 150, 0xC0FFEE);
    let h = compute_histograms(&img);

    let total = (200 * 150) as u32;
    assert_eq!(h.r.iter().sum::<u32>(), total);
    assert_eq!(h.g.iter().sum::<u32>(), total);
    assert_eq!(h.b.iter().sum::<u32>(), total);
}

#[test]
fn histogram_bins_by_the_high_byte() {
    let img = solid(10, 10, [u8_to_u16(128), u8_to_u16(0), u8_to_u16(255), u16::MAX]);
    let h = compute_histograms(&img);

    // 128 * 257 = 32896, and 32896 >> 8 = 128.
    assert_eq!(h.r[128], 100);
    assert_eq!(h.g[0], 100);
    assert_eq!(h.b[255], 100);
}

#[test]
fn histogram_channels_are_not_crossed() {
    let img = solid(4, 4, [u8_to_u16(10), u8_to_u16(20), u8_to_u16(30), u16::MAX]);
    let h = compute_histograms(&img);

    assert_eq!(h.r[10], 16, "red");
    assert_eq!(h.g[20], 16, "green");
    assert_eq!(h.b[30], 16, "blue");
    assert_eq!(h.r[20] + h.r[30], 0, "red bin picked up another channel");
}

#[test]
fn histograms_describe_the_full_resolution_image() {
    // Larger than the preview cap, so preview and full-res dimensions differ
    // and we can tell which one was counted.
    let img = solid(2500, 100, [u8_to_u16(128); 4]);
    let prepared = prepare_image(&img);

    assert_eq!(prepared.full_width, 2500);
    assert_eq!(prepared.preview_width, PREVIEW_MAX_EDGE, "preview is capped");

    assert_eq!(prepared.histograms.r.iter().sum::<u32>(), 2500 * 100);
    assert_eq!(prepared.histograms.r[128], 2500 * 100);
}

// ── Preview preparation ───────────────────────────────────────────────────────

#[test]
fn preview_uses_the_same_linearisation_as_export() {
    // For an image inside the preview cap there is no resampling, so the
    // preview bytes must be exactly what the export path would upload. This is
    // the guarantee that sharing `linearize_rgba_u16` was supposed to buy.
    let img = rgb_ramp(97, 13);
    let prepared = prepare_image(&img);

    let lut = build_srgb_to_linear_lut_u16();
    let expected = linearize_rgba_u16(img.as_raw(), &lut);

    assert_eq!(
        prepared.preview_pixels, expected,
        "preview and export linearisation diverged"
    );
}

#[test]
fn prepare_reports_both_resolutions() {
    let img = gray_ramp(3000, 1500);
    let prepared = prepare_image(&img);

    assert_eq!((prepared.full_width, prepared.full_height), (3000, 1500));
    assert_eq!((prepared.preview_width, prepared.preview_height), (2048, 1024));
    assert_eq!(
        prepared.preview_pixels.len(),
        2048 * 1024 * LINEAR_BYTES_PER_PIXEL
    );
}

// ── Payload framing ───────────────────────────────────────────────────────────

#[test]
fn payload_round_trips_through_the_frontend_layout() {
    let img = rgb_ramp(9, 5);
    let prepared = prepare_image(&img);
    let payload = build_payload(&prepared);

    assert_eq!(payload.len(), payload_len_for(9, 5));

    // Parse exactly the way openImage.ts does.
    let width = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    assert_eq!((width, height), (9, 5));

    let pixel_bytes = width as usize * height as usize * LINEAR_BYTES_PER_PIXEL;
    assert_eq!(
        &payload[PAYLOAD_HEADER_BYTES..PAYLOAD_HEADER_BYTES + pixel_bytes],
        &prepared.preview_pixels[..]
    );

    let mut offset = PAYLOAD_HEADER_BYTES + pixel_bytes;
    for (channel, expected) in [
        ("r", &prepared.histograms.r),
        ("g", &prepared.histograms.g),
        ("b", &prepared.histograms.b),
    ] {
        let mut bins = [0u32; HIST_BINS];
        for (i, bin) in bins.iter_mut().enumerate() {
            let at = offset + i * 4;
            *bin = u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        }
        assert_eq!(bins, *expected, "{channel} histogram misplaced in payload");
        assert_eq!(bins.iter().sum::<u32>(), 9 * 5, "{channel} pixel count");
        offset += HIST_BINS * 4;
    }

    assert_eq!(offset, payload.len(), "trailing bytes in payload");
}

#[test]
fn payload_header_carries_preview_not_source_dimensions() {
    // openImage.ts sizes its pixel view from the header, so the header must
    // describe the preview — using the source size would desynchronise the
    // histogram offset and corrupt the texture.
    let img = solid(2500, 500, [0, 0, 0, u16::MAX]);
    let prepared = prepare_image(&img);
    let payload = build_payload(&prepared);

    let width = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(payload[4..8].try_into().unwrap());

    assert_eq!((width, height), (2048, 410));
    assert_eq!(payload.len(), payload_len_for(width, height));
}
