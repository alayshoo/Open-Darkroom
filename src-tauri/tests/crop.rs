// src-tauri/tests/crop.rs
//
// The crop: its geometry, the export rendering only what it keeps, and the
// input histograms binning only what it keeps.

mod common;

use common::{noise, rgb_ramp};

use open_darkroom_lib::crop::{crop_view, crop_view_to_bytes, CropPayload, CROP_VIEW_BYTES};
use open_darkroom_lib::export_rendering::{
    render_cropped_with_chunk_rows, render_image_with_chunk_rows, SlidersPayload,
};
use open_darkroom_lib::icc::SourceSpace;
use open_darkroom_lib::image_opening::{compute_histograms, compute_histograms_in, Rgba16Image};
use open_darkroom_lib::output_space::OutputSpace;

fn render(img: &Rgba16Image, sliders: &SlidersPayload, crop: &CropPayload, chunk_rows: u32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_cropped_with_chunk_rows(
        img.as_raw(),
        w,
        h,
        4,
        sliders,
        crop,
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        chunk_rows,
        |_| {},
    ))
    .expect("render")
}

/// The rows and columns `view` keeps, cut out of a full RGB8 render.
fn cut(full: &[u8], full_width: u32, x0: u32, y0: u32, width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for y in y0..y0 + height {
        let at = ((y * full_width + x0) * 3) as usize;
        out.extend_from_slice(&full[at..at + (width * 3) as usize]);
    }
    out
}

#[test]
fn the_full_frame_maps_one_to_one() {
    let view = crop_view(&CropPayload::FULL_FRAME, 640, 480);
    assert_eq!((view.width, view.height), (640, 480));
    assert_eq!(view.origin, [0.0, 0.0]);
    assert_eq!(view.source_texels(0..480), [0, 0, 640, 480]);
}

/// Mirrors the same case in tests/unit/crop.test.ts, so the two snap alike.
#[test]
fn an_unrotated_crop_snaps_to_whole_pixels_as_the_frontend_does() {
    let crop = CropPayload { cx: 0.4, cy: 0.5, w: 0.333, h: 0.5, angle: 0.0 };
    let view = crop_view(&crop, 100, 10);
    assert_eq!(view.origin, [23.0, 3.0]);
    assert_eq!((view.width, view.height), (34, 5));
}

#[test]
fn a_malformed_crop_falls_back_to_the_full_frame() {
    let crop = CropPayload { w: f64::NAN, ..CropPayload::FULL_FRAME };
    assert_eq!(crop_view(&crop, 30, 20), crop_view(&CropPayload::FULL_FRAME, 30, 20));
}

#[test]
fn the_uniform_matches_its_declared_size() {
    let bytes = crop_view_to_bytes(&crop_view(&CropPayload::FULL_FRAME, 8, 4));
    assert_eq!(bytes.len(), CROP_VIEW_BYTES);
    let size_x = f32::from_le_bytes(bytes[24..28].try_into().unwrap());
    assert_eq!(size_x, 8.0);
}

#[test]
fn an_export_keeps_exactly_the_pixels_it_covers() {
    let img = rgb_ramp(64, 40);
    let sliders = SlidersPayload::default();
    let crop = CropPayload { cx: 0.5, cy: 0.5, w: 0.5, h: 0.5, angle: 0.0 };
    let view = crop_view(&crop, 64, 40);

    let full = render(&img, &sliders, &CropPayload::FULL_FRAME, 40);
    let cropped = render(&img, &sliders, &crop, 40);

    assert_eq!((view.width, view.height), (32, 20));
    assert_eq!(cropped, cut(&full, 64, 16, 10, 32, 20));
}

/// The blurs run over a margin around the crop, so its edges see the same
/// neighbours a full render gives them, whatever the chunking.
#[test]
fn sharpening_near_the_crop_edge_matches_the_full_render() {
    let img = noise(300, 260, 7);
    let sliders = SlidersPayload {
        clarity: 100.0,
        texture: 60.0,
        usm_amount: 150.0,
        usm_radius: 3.0,
        ..Default::default()
    };
    let crop = CropPayload { cx: 0.45, cy: 0.55, w: 0.4, h: 0.5, angle: 0.0 };
    let view = crop_view(&crop, 300, 260);
    let [x0, y0] = view.origin.map(|v| v as u32);

    let full = pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        300,
        260,
        4,
        &sliders,
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        260,
        |_| {},
    ))
    .expect("render");
    let expected = cut(&full, 300, x0, y0, view.width, view.height);

    for chunk_rows in [view.height, 17, 64] {
        assert_eq!(
            render(&img, &sliders, &crop, chunk_rows),
            expected,
            "cropped render differs at {chunk_rows} rows per chunk"
        );
    }
}

#[test]
fn a_rotated_crop_renders_at_its_own_size() {
    let img = rgb_ramp(80, 60);
    let crop = CropPayload { cx: 0.5, cy: 0.5, w: 0.5, h: 0.5, angle: 0.1 };
    let view = crop_view(&crop, 80, 60);
    let out = render(&img, &SlidersPayload::default(), &crop, 7);
    assert_eq!(out.len(), (view.width * view.height * 3) as usize);
}

#[test]
fn the_input_histograms_bin_only_what_the_crop_keeps() {
    let img = rgb_ramp(64, 8);
    let channels = 4;

    let whole = compute_histograms(img.as_raw(), channels);
    let full_view = crop_view(&CropPayload::FULL_FRAME, 64, 8);
    let whole_in = compute_histograms_in(img.as_raw(), 64, 8, channels, &full_view);
    assert_eq!(whole.r, whole_in.r);
    assert_eq!(whole.b, whole_in.b);

    // The left half of a ramp rising left to right holds none of its top values.
    let left = CropPayload { cx: 0.25, cy: 0.5, w: 0.5, h: 1.0, angle: 0.0 };
    let view = crop_view(&left, 64, 8);
    let h = compute_histograms_in(img.as_raw(), 64, 8, channels, &view);
    assert_eq!(h.r.iter().sum::<u32>(), 32 * 8);
    assert_eq!(h.r[255], 0);
    assert!(whole.r[255] > 0);
}
