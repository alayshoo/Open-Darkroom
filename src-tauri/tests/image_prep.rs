// src-tauri/tests/image_prep.rs
//
// Layer 1: everything between "the file is decoded" and "the frontend has a
// texture" — downscaling, histograms, and the payload wire format.

mod common;

use common::{gray_ramp, linear_to_srgb, noise, rgb_ramp, solid, u8_to_u16};

use half::f16;
use image::{ImageBuffer, Rgba};
use open_darkroom_lib::image_opening::Rgba16Image;

use open_darkroom_lib::color::{build_srgb_to_linear_lut_u16, linearize_u16, LINEAR_BYTES_PER_PIXEL};
use open_darkroom_lib::image_opening::{
    build_payload, compute_histograms, downscale_to_max_edge, payload_len_for, prepare_image,
    PreparedImage, HIST_BINS, PAYLOAD_HEADER_BYTES, PREVIEW_MAX_EDGE,
};

/// Prepare an RGBA fixture, which is the shape a source declaring alpha decodes to.
fn prepare(img: &Rgba16Image) -> PreparedImage {
    let (w, h) = img.dimensions();
    prepare_image(img.as_raw(), w, h, 4)
}

/// Downscale an RGBA fixture, discarding the dimensions the caller already knows.
fn downscale(img: &Rgba16Image, max_edge: u32) -> (Vec<u16>, u32, u32) {
    let (w, h) = img.dimensions();
    downscale_to_max_edge(img.as_raw(), w, h, 4, max_edge)
}

/// The same pixels as `img`, with the alpha channel dropped.
fn without_alpha(img: &Rgba16Image) -> Vec<u16> {
    img.as_raw()
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect()
}

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
        let (_, new_w, new_h) = downscale(&img, PREVIEW_MAX_EDGE);
        assert_eq!((new_w, new_h), expected, "downscaling {w}×{h}");

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
    let (same, w, h) = downscale(&img, PREVIEW_MAX_EDGE);

    assert_eq!((w, h), (64, 32));
    assert_eq!(&same, img.as_raw(), "no resampling should occur");

    // Exactly at the limit is still untouched.
    let edge = solid(PREVIEW_MAX_EDGE, 100, [7, 7, 7, u16::MAX]);
    let (_, w, h) = downscale(&edge, PREVIEW_MAX_EDGE);
    assert_eq!((w, h), (PREVIEW_MAX_EDGE, 100));
}

#[test]
fn downscale_preserves_a_flat_colour_exactly() {
    // Resampling a constant image must be a no-op on the values. This is what
    // catches a botched u16 byte re-interpretation in the resize round trip.
    let img = solid(4000, 2000, [1234, 5678, 9012, u16::MAX]);
    let (small, _, _) = downscale(&img, PREVIEW_MAX_EDGE);

    for px in small.chunks_exact(4) {
        assert_eq!([px[0], px[1], px[2]], [1234, 5678, 9012]);
    }
}

#[test]
fn downscale_keeps_channels_distinct() {
    // A channel swap during the byte round trip would survive the flat-colour
    // test, so check a gradient where the channels disagree.
    let img = rgb_ramp(3000, 100);
    let (small, w, _) = downscale(&img, PREVIEW_MAX_EDGE);

    let first = &small[0..3];
    let last = &small[(w as usize - 1) * 4..(w as usize - 1) * 4 + 3];

    assert!(last[0] > first[0], "red ramps up");
    assert!(last[1] < first[1], "green ramps down");
}

/// Resizing a three-channel source uses a different `fast_image_resize` pixel
/// type, and unlike the four-channel one it does no alpha premultiply round trip
/// at all. With an opaque source the two have to land on the same pixels.
#[test]
fn downscale_agrees_across_channel_counts() {
    let img = rgb_ramp(3000, 100);

    let (from_rgba, w, h) = downscale(&img, PREVIEW_MAX_EDGE);
    let (from_rgb, rgb_w, rgb_h) =
        downscale_to_max_edge(&without_alpha(&img), 3000, 100, 3, PREVIEW_MAX_EDGE);

    assert_eq!((rgb_w, rgb_h), (w, h));

    let stripped: Vec<u16> = from_rgba
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect();
    assert_eq!(
        from_rgb, stripped,
        "the resample changed when the alpha channel was dropped"
    );
}

// ── Histograms ────────────────────────────────────────────────────────────────

#[test]
fn histogram_counts_every_pixel_once() {
    let img = noise(200, 150, 0xC0FFEE);
    let h = compute_histograms(img.as_raw(), 4);

    let total = (200 * 150) as u32;
    assert_eq!(h.r.iter().sum::<u32>(), total);
    assert_eq!(h.g.iter().sum::<u32>(), total);
    assert_eq!(h.b.iter().sum::<u32>(), total);
}

#[test]
fn histogram_bins_by_the_high_byte() {
    let img = solid(10, 10, [u8_to_u16(128), u8_to_u16(0), u8_to_u16(255), u16::MAX]);
    let h = compute_histograms(img.as_raw(), 4);

    // 128 * 257 = 32896, and 32896 >> 8 = 128.
    assert_eq!(h.r[128], 100);
    assert_eq!(h.g[0], 100);
    assert_eq!(h.b[255], 100);
}

#[test]
fn histogram_channels_are_not_crossed() {
    let img = solid(4, 4, [u8_to_u16(10), u8_to_u16(20), u8_to_u16(30), u16::MAX]);
    let h = compute_histograms(img.as_raw(), 4);

    assert_eq!(h.r[10], 16, "red");
    assert_eq!(h.g[20], 16, "green");
    assert_eq!(h.b[30], 16, "blue");
    assert_eq!(h.r[20] + h.r[30], 0, "red bin picked up another channel");
}

/// The stride is the only thing separating one channel's bin from the next, so a
/// three-channel source has to bin exactly as its RGBA equivalent does.
#[test]
fn histograms_do_not_depend_on_the_channel_count() {
    let img = noise(64, 48, 0xA11FA);

    let rgba = compute_histograms(img.as_raw(), 4);
    let rgb = compute_histograms(&without_alpha(&img), 3);

    assert_eq!(rgb.r, rgba.r, "red");
    assert_eq!(rgb.g, rgba.g, "green");
    assert_eq!(rgb.b, rgba.b, "blue");
}

#[test]
fn histograms_describe_the_full_resolution_image() {
    // Larger than the preview cap, so preview and full-res dimensions differ
    // and we can tell which one was counted.
    let img = solid(2500, 100, [u8_to_u16(128); 4]);
    let prepared = prepare(&img);

    assert_eq!(prepared.full_width, 2500);
    assert_eq!(prepared.preview_width, PREVIEW_MAX_EDGE, "preview is capped");

    assert_eq!(prepared.histograms.r.iter().sum::<u32>(), 2500 * 100);
    assert_eq!(prepared.histograms.r[128], 2500 * 100);
}

// ── Preview preparation ───────────────────────────────────────────────────────

#[test]
fn preview_uses_the_shared_linearisation() {
    // For an image inside the preview cap there is no resampling, so the preview
    // bytes must be exactly the shared conversion's output. The export uploads
    // the same curve at f32 — `the_two_tables_describe_the_same_curve` in
    // color.rs is what holds the two widths to one tone scale.
    let img = rgb_ramp(97, 13);
    let prepared = prepare(&img);

    let lut = build_srgb_to_linear_lut_u16();
    let expected = linearize_u16(img.as_raw(), 4, &lut);

    assert_eq!(
        prepared.preview_pixels, expected,
        "preview and export linearisation diverged"
    );
}

/// Dropping the stored alpha must not disturb anything the frontend receives.
/// Both branches of `prepare_image` are covered: one fixture sits inside the
/// preview cap and takes the direct path, the other is resampled.
#[test]
fn preparing_a_three_channel_source_matches_its_rgba_equivalent() {
    for (w, h) in [(97u32, 13u32), (3000, 200)] {
        let img = rgb_ramp(w, h);

        let from_rgba = prepare(&img);
        let from_rgb = prepare_image(&without_alpha(&img), w, h, 3);

        assert_eq!(
            (from_rgb.preview_width, from_rgb.preview_height),
            (from_rgba.preview_width, from_rgba.preview_height),
            "{w}×{h}: preview dimensions"
        );
        assert_eq!(
            from_rgb.preview_pixels, from_rgba.preview_pixels,
            "{w}×{h}: preview pixels diverged when alpha was dropped"
        );
        assert_eq!(from_rgb.histograms.r, from_rgba.histograms.r, "{w}×{h}: red");
        assert_eq!(from_rgb.histograms.g, from_rgba.histograms.g, "{w}×{h}: green");
        assert_eq!(from_rgb.histograms.b, from_rgba.histograms.b, "{w}×{h}: blue");
    }
}

/// The preview is resampled, and resampling averages light, not code values.
///
/// A checkerboard is the sharpest statement of the difference: half the pixels
/// are black and half are white, so the honest average is linear 0.5 whatever
/// the block size. Averaged in the sRGB encoding instead it comes out at linear
/// 0.21 — most of a stop dark — and the error lands in exactly the fine texture
/// the preview is there to predict.
#[test]
fn the_preview_is_resampled_in_linear_light() {
    // Long edge past the cap so a resample actually happens. Every 2×2 block
    // holds two black and two white pixels.
    let width = PREVIEW_MAX_EDGE * 2;
    let img: Rgba16Image = ImageBuffer::from_fn(width, 8, |x, y| {
        let v = if (x + y) % 2 == 0 { 255u8 } else { 0u8 };
        Rgba([u8_to_u16(v), u8_to_u16(v), u8_to_u16(v), u16::MAX])
    });

    let prepared = prepare(&img);
    assert_eq!(
        (prepared.preview_width, prepared.preview_height),
        (PREVIEW_MAX_EDGE, 4),
        "the fixture must actually be downscaled for this to prove anything"
    );

    // Sample the interior: the edge columns average fewer neighbours.
    let mean: f64 = prepared
        .preview_pixels
        .chunks_exact(LINEAR_BYTES_PER_PIXEL)
        .skip(8)
        .take(prepared.preview_width as usize - 16)
        .map(|px| f16::from_le_bytes([px[2], px[3]]).to_f32() as f64)
        .sum::<f64>()
        / (prepared.preview_width as usize - 16) as f64;

    assert!(
        (mean - 0.5).abs() < 0.02,
        "checkerboard averaged to linear {mean:.4} (sRGB {:.0}/255); \
         averaging in light gives 0.5 (sRGB 188), averaging the sRGB encoding \
         gives 0.21 (sRGB 128)",
        linear_to_srgb(mean) * 255.0
    );
}

#[test]
fn prepare_reports_both_resolutions() {
    let img = gray_ramp(3000, 1500);
    let prepared = prepare(&img);

    assert_eq!((prepared.full_width, prepared.full_height), (3000, 1500));
    assert_eq!((prepared.preview_width, prepared.preview_height), (2048, 1024));
    assert_eq!(
        prepared.preview_pixels.len(),
        2048 * 1024 * LINEAR_BYTES_PER_PIXEL
    );
}

// ── Payload framing ───────────────────────────────────────────────────────────

/// The header carries both resolutions so the frontend can tell how far the
/// preview sits from full size. A slider measured in image pixels needs that
/// ratio, and the preview buffer alone cannot supply it.
#[test]
fn payload_header_carries_the_full_resolution_of_a_downscaled_preview() {
    let img = gray_ramp(3000, 1500);
    let prepared = prepare(&img);
    let payload = build_payload(&prepared);

    let preview_width = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let preview_height = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    let full_width = u32::from_le_bytes(payload[8..12].try_into().unwrap());
    let full_height = u32::from_le_bytes(payload[12..16].try_into().unwrap());

    assert_eq!((preview_width, preview_height), (2048, 1024));
    assert_eq!((full_width, full_height), (3000, 1500));
    assert_ne!(
        preview_width, full_width,
        "this fixture is only meaningful while the preview is downscaled"
    );
}

#[test]
fn payload_round_trips_through_the_frontend_layout() {
    let img = rgb_ramp(9, 5);
    let prepared = prepare(&img);
    let payload = build_payload(&prepared);

    assert_eq!(payload.len(), payload_len_for(9, 5));

    // Parse exactly the way openImage.ts does.
    let width = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(payload[4..8].try_into().unwrap());
    let full_width = u32::from_le_bytes(payload[8..12].try_into().unwrap());
    let full_height = u32::from_le_bytes(payload[12..16].try_into().unwrap());
    assert_eq!((width, height), (9, 5));
    assert_eq!((full_width, full_height), (9, 5));

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
    let prepared = prepare(&img);
    let payload = build_payload(&prepared);

    let width = u32::from_le_bytes(payload[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(payload[4..8].try_into().unwrap());

    assert_eq!((width, height), (2048, 410));
    assert_eq!(payload.len(), payload_len_for(width, height));
}
