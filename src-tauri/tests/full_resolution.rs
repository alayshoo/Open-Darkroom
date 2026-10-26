// src-tauri/tests/full_resolution.rs
//
// Layer 1: the bands of rows the preview's full-resolution texture is streamed
// from. They have to agree with the preview built at open, which is what the
// view shows until they arrive.

mod common;

use common::{noise, rgb_ramp};

use half::f16;
use open_darkroom_lib::full_resolution::{
    build_rows_payload, served_size, FullResolution, Rows, ROWS_HEADER_BYTES,
};
use open_darkroom_lib::icc::SourceSpace;
use open_darkroom_lib::image_opening::{prepare_image, PREVIEW_MAX_EDGE};

/// Every band of `rows` rows, end to end, and the size they report.
fn all_bands(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    max_edge: u32,
    rows: u32,
) -> (u32, u32, Vec<u8>) {
    let full = FullResolution::default();
    let space = SourceSpace::default();
    let mut out = Vec::new();
    let mut y = 0;
    let mut size = (0, 0);
    loop {
        let band = full
            .rows(src, width, height, channels, &space, max_edge, y, rows)
            .expect("band");
        assert_eq!(band.y, y);
        size = (band.width, band.height);
        out.extend_from_slice(&band.pixels);
        y += band.rows;
        if y >= band.height {
            break;
        }
    }
    (size.0, size.1, out)
}

#[test]
fn an_image_within_the_limit_is_served_whole_and_matches_its_preview() {
    let img = noise(300, 77, 3);
    let (w, h) = img.dimensions();
    let prepared = prepare_image(img.as_raw(), w, h, 4, &SourceSpace::default()).unwrap();

    let (sw, sh, pixels) = all_bands(img.as_raw(), w, h, 4, 4096, 16);
    assert_eq!((sw, sh), (w, h));
    assert_eq!(pixels, prepared.preview_pixels);
}

#[test]
fn bands_of_any_height_join_up_to_the_same_image() {
    let img = rgb_ramp(64, 50);
    let (w, h) = img.dimensions();
    let (_, _, whole) = all_bands(img.as_raw(), w, h, 4, 4096, 1000);
    for rows in [1, 7, 49, 50] {
        assert_eq!(
            all_bands(img.as_raw(), w, h, 4, 4096, rows).2,
            whole,
            "{rows} rows a band"
        );
    }
}

#[test]
fn the_last_band_is_cut_short_at_the_bottom() {
    let img = rgb_ramp(8, 10);
    let band = FullResolution::default()
        .rows(
            img.as_raw(),
            8,
            10,
            4,
            &SourceSpace::default(),
            4096,
            6,
            100,
        )
        .unwrap();
    assert_eq!(band.rows, 4);
    assert_eq!(band.pixels.len(), 8 * 4 * 8);
}

#[test]
fn a_band_past_the_bottom_is_refused() {
    let img = rgb_ramp(8, 10);
    let result = FullResolution::default().rows(
        img.as_raw(),
        8,
        10,
        4,
        &SourceSpace::default(),
        4096,
        10,
        1,
    );
    assert!(result.is_err());
}

#[test]
fn a_source_without_alpha_is_served_opaque() {
    let img = rgb_ramp(16, 4);
    let rgb: Vec<u16> = img
        .as_raw()
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let (_, _, from_rgb) = all_bands(&rgb, 16, 4, 3, 4096, 3);
    for px in from_rgb.chunks_exact(8) {
        assert_eq!(f16::from_le_bytes([px[6], px[7]]), f16::ONE);
    }
    let (_, _, from_rgba) = all_bands(img.as_raw(), 16, 4, 4, 4096, 3);
    assert_eq!(from_rgb, from_rgba);
}

#[test]
fn served_size_keeps_an_image_that_fits_and_shrinks_one_that_does_not() {
    assert_eq!(served_size(6000, 4000, 8192), (6000, 4000));
    assert_eq!(served_size(8192, 100, 8192), (8192, 100));
    assert_eq!(served_size(20000, 10000, 16384), (16384, 8192));
    assert_eq!(served_size(3000, 9000, 4500), (1500, 4500));
}

#[test]
fn an_image_past_the_limit_is_resampled_as_the_preview_is() {
    // Fitted to the preview's own edge, the served image is the preview.
    let img = noise(PREVIEW_MAX_EDGE + 300, 40, 9);
    let (w, h) = img.dimensions();
    let prepared = prepare_image(img.as_raw(), w, h, 4, &SourceSpace::default()).unwrap();

    let (sw, sh, pixels) = all_bands(img.as_raw(), w, h, 4, PREVIEW_MAX_EDGE, 9);
    assert_eq!((sw, sh), (prepared.preview_width, prepared.preview_height));
    assert_eq!(pixels, prepared.preview_pixels);
}

#[test]
fn a_second_texture_limit_for_the_same_image_is_refused() {
    let img = rgb_ramp(100, 10);
    let full = FullResolution::default();
    let space = SourceSpace::default();
    full.rows(img.as_raw(), 100, 10, 4, &space, 50, 0, 1)
        .unwrap();
    assert!(full
        .rows(img.as_raw(), 100, 10, 4, &space, 40, 0, 1)
        .is_err());
    // Whole-image serving is unaffected.
    full.rows(img.as_raw(), 100, 10, 4, &space, 4096, 0, 1)
        .unwrap();
}

#[test]
fn the_payload_carries_the_size_and_band_before_the_pixels() {
    let rows = Rows {
        width: 3,
        height: 9,
        y: 4,
        rows: 2,
        pixels: vec![7u8; 3 * 2 * 8],
    };
    let payload = build_rows_payload(&rows);
    assert_eq!(payload.len(), ROWS_HEADER_BYTES + rows.pixels.len());
    let word = |i: usize| u32::from_le_bytes(payload[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!([word(0), word(1), word(2), word(3)], [3, 9, 4, 2]);
    assert!(payload[ROWS_HEADER_BYTES..].iter().all(|&b| b == 7));
}
