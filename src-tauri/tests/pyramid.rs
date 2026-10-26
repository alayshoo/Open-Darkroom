// src-tauri/tests/pyramid.rs
//
// Layer 1: the tile pyramid — its level sizes, how tiles are cut, and that what
// they carry is the image the export renders.

mod common;

use common::{noise, solid, u8_to_u16};

use half::f16;
use image::{ImageBuffer, Rgba};
use std::sync::Arc;

use open_darkroom_lib::color::{linearize_u16, LINEAR_BYTES_PER_PIXEL};
use open_darkroom_lib::icc::SourceSpace;
use open_darkroom_lib::image_opening::{Rgba16Image, PREVIEW_MAX_EDGE};
use open_darkroom_lib::pyramid::{
    build_tile_payload, level_sizes, LevelSize, Pyramid, TILE_HEADER_BYTES, TILE_SIZE,
};

fn pyramid_of(img: &Rgba16Image) -> Arc<Pyramid> {
    pyramid_in(
        img.as_raw().clone(),
        img.width(),
        img.height(),
        4,
        SourceSpace::default(),
    )
}

fn pyramid_in(src: Vec<u16>, w: u32, h: u32, channels: usize, space: SourceSpace) -> Arc<Pyramid> {
    Arc::new(Pyramid::new(Arc::new(src), w, h, channels, space))
}

/// Every tile of a level, stitched back into one RGBA f16 buffer.
fn stitch(pyramid: &Pyramid, level: usize) -> Vec<u8> {
    let size = pyramid.sizes()[level];
    let stride = size.width as usize * LINEAR_BYTES_PER_PIXEL;
    let mut out = vec![0u8; stride * size.height as usize];
    for row in 0..size.rows() {
        for column in 0..size.columns() {
            let tile = pyramid.tile(level, column, row).expect("tile");
            let tile_stride = tile.width as usize * LINEAR_BYTES_PER_PIXEL;
            for y in 0..tile.height as usize {
                let at = (row * TILE_SIZE) as usize + y;
                let x = (column * TILE_SIZE) as usize * LINEAR_BYTES_PER_PIXEL;
                out[at * stride + x..at * stride + x + tile_stride]
                    .copy_from_slice(&tile.pixels[y * tile_stride..(y + 1) * tile_stride]);
            }
        }
    }
    out
}

fn channel(pixels: &[u8], pixel: usize, c: usize) -> f32 {
    let at = pixel * LINEAR_BYTES_PER_PIXEL + c * 2;
    f16::from_le_bytes([pixels[at], pixels[at + 1]]).to_f32()
}

fn lut_profile() -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/srgb-lut.icc");
    std::fs::read(path).expect("tests/fixtures/srgb-lut.icc — regenerate with the example")
}

// ── Level sizes ───────────────────────────────────────────────────────────────

#[test]
fn an_image_within_the_preview_edge_is_a_single_level() {
    for (w, h) in [
        (1, 1),
        (640, 480),
        (PREVIEW_MAX_EDGE, 1000),
        (1000, PREVIEW_MAX_EDGE),
    ] {
        assert_eq!(
            level_sizes(w, h),
            vec![LevelSize {
                width: w,
                height: h
            }],
            "{w}×{h}"
        );
    }
}

#[test]
fn each_level_halves_the_one_below_until_the_preview_edge_is_reached() {
    let sizes = level_sizes(9504, 6336);
    let edges: Vec<_> = sizes.iter().map(|s| (s.width, s.height)).collect();
    assert_eq!(
        edges,
        [(9504, 6336), (4752, 3168), (2376, 1584), (1188, 792)]
    );
}

#[test]
fn odd_sizes_round_up_so_every_source_pixel_has_a_parent() {
    let sizes = level_sizes(4097, 3);
    let edges: Vec<_> = sizes.iter().map(|s| (s.width, s.height)).collect();
    assert_eq!(edges, [(4097, 3), (2049, 2), (1025, 1)]);
}

#[test]
fn only_the_top_level_fits_the_preview_edge() {
    for (w, h) in [(2049, 10), (4096, 4096), (12000, 800), (300, 20000)] {
        let sizes = level_sizes(w, h);
        let (top, below) = sizes.split_last().unwrap();
        assert!(
            top.width.max(top.height) <= PREVIEW_MAX_EDGE,
            "{w}×{h}: top is {top:?}"
        );
        for s in below {
            assert!(
                s.width.max(s.height) > PREVIEW_MAX_EDGE,
                "{w}×{h}: {s:?} stops too late"
            );
        }
    }
}

// ── Cutting tiles ─────────────────────────────────────────────────────────────

#[test]
fn edge_tiles_are_cut_short_by_what_is_left_of_the_level() {
    let pyramid = pyramid_of(&solid(1300, 600, [0, 0, 0, u16::MAX]));
    let size = pyramid.sizes()[0];
    assert_eq!((size.columns(), size.rows()), (3, 2));

    let corner = pyramid.tile(0, 2, 1).expect("corner tile");
    assert_eq!(
        (corner.width, corner.height),
        (1300 - 2 * TILE_SIZE, 600 - TILE_SIZE)
    );
    assert_eq!(
        corner.pixels.len(),
        (corner.width * corner.height) as usize * 8
    );

    let full = pyramid.tile(0, 0, 0).expect("interior tile");
    assert_eq!((full.width, full.height), (TILE_SIZE, TILE_SIZE));
}

#[test]
fn a_tile_off_the_level_is_refused() {
    let pyramid = pyramid_of(&solid(1300, 600, [0, 0, 0, u16::MAX]));
    assert!(pyramid.tile(0, 3, 0).is_err());
    assert!(pyramid.tile(0, 0, 2).is_err());
    assert!(
        pyramid.tile(1, 0, 0).is_err(),
        "a single-level pyramid has no level 1"
    );
}

// ── What the tiles carry ──────────────────────────────────────────────────────

/// Level 0 is what the export uploads, cut into pieces: the same tables, the
/// same bytes, nothing resampled.
#[test]
fn full_resolution_tiles_are_the_source_linearised_exactly() {
    let img = noise(1100, 530, 7);
    let pyramid = pyramid_of(&img);
    let luts = SourceSpace::default().luts(f16::from_f32);
    assert_eq!(stitch(&pyramid, 0), linearize_u16(img.as_raw(), 4, &luts));
}

#[test]
fn a_source_without_alpha_tiles_fully_opaque() {
    let img = noise(700, 40, 3);
    let rgb: Vec<u16> = img
        .as_raw()
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let pyramid = pyramid_in(rgb, 700, 40, 3, SourceSpace::default());

    let with_alpha = pyramid_of(&img);
    assert_eq!(stitch(&pyramid, 0), stitch(&with_alpha, 0));
}

#[test]
fn a_baked_source_tiles_as_its_converted_pixels() {
    let space = SourceSpace::resolve(Some(lut_profile()));
    assert!(space.baked, "the fixture should take the baked path");

    let img = noise(600, 30, 11);
    let pyramid = pyramid_in(img.as_raw().clone(), 600, 30, 4, space.clone());
    let baked = space.bake(img.as_raw(), 4).unwrap().unwrap();
    assert_eq!(
        stitch(&pyramid, 0),
        linearize_u16(&baked, 4, &space.luts(f16::from_f32))
    );
}

#[test]
fn a_flat_image_stays_flat_at_every_level() {
    let v = u8_to_u16(128);
    let pyramid = pyramid_of(&solid(4200, 30, [v, v, v, u16::MAX]));
    let expected = channel(&stitch(&pyramid, 0), 0, 0);

    for level in 1..pyramid.sizes().len() {
        let pixels = stitch(&pyramid, level);
        for p in 0..pixels.len() / LINEAR_BYTES_PER_PIXEL {
            let got = channel(&pixels, p, 0);
            assert!(
                (got - expected).abs() < 1e-4,
                "level {level} pixel {p}: {got} vs {expected}"
            );
        }
    }
}

/// Averaging encoded values would land a black and white checkerboard on sRGB
/// mid-grey, linear 0.21. The light those pixels stand for averages to 0.5.
#[test]
fn reduced_levels_average_light_not_its_encoding() {
    let img: Rgba16Image = ImageBuffer::from_fn(4200, 32, |x, y| {
        let v = if (x + y) % 2 == 0 { u16::MAX } else { 0 };
        Rgba([v, v, v, u16::MAX])
    });
    let pyramid = pyramid_of(&img);

    let pixels = stitch(&pyramid, 1);
    let size = pyramid.sizes()[1];
    // Clear of the edges, where the filter's reach is cut short.
    let centre = (size.height / 2 * size.width + size.width / 2) as usize;
    let got = channel(&pixels, centre, 1);
    assert!((got - 0.5).abs() < 0.01, "checkerboard reduced to {got}");
}

#[test]
fn a_level_is_the_same_whichever_order_the_levels_are_built_in() {
    let img = noise(4300, 24, 5);
    let finest_first = pyramid_of(&img);
    let coarsest_first = pyramid_of(&img);

    let a = (stitch(&finest_first, 1), stitch(&finest_first, 2));
    let b2 = stitch(&coarsest_first, 2);
    let b1 = stitch(&coarsest_first, 1);
    assert_eq!(a, (b1, b2));
}

// ── Prefetching ───────────────────────────────────────────────────────────────

#[test]
fn reduced_levels_are_only_built_when_asked_for() {
    let pyramid = pyramid_of(&noise(4300, 16, 2));
    assert!(
        pyramid.is_ready(0),
        "level 0 is read straight from the source"
    );
    assert!(!pyramid.is_ready(1) && !pyramid.is_ready(2));

    pyramid.tile(2, 0, 0).expect("tile");
    assert!(pyramid.is_ready(2));
    assert!(
        !pyramid.is_ready(1),
        "building one level must not build another"
    );
}

#[test]
fn prefetching_builds_the_levels_either_side() {
    let pyramid = pyramid_of(&noise(8700, 16, 9));
    assert_eq!(pyramid.sizes().len(), 4);

    pyramid.tile(2, 0, 0).expect("tile");
    let builds = pyramid.prefetch_around(2);
    assert_eq!(builds.len(), 2, "levels 1 and 3");
    builds.into_iter().for_each(|b| b.join().unwrap());

    assert!(pyramid.is_ready(1) && pyramid.is_ready(3));
    assert!(
        pyramid.prefetch_around(2).is_empty(),
        "nothing left to build either side"
    );
}

#[test]
fn prefetching_skips_full_resolution_and_the_ends_of_the_pyramid() {
    let pyramid = pyramid_of(&noise(4300, 16, 4));
    let top = pyramid.sizes().len() - 1;

    let builds = pyramid.prefetch_around(0);
    assert_eq!(builds.len(), 1, "only level 1 sits beside full resolution");
    builds.into_iter().for_each(|b| b.join().unwrap());

    assert!(
        pyramid.prefetch_around(top).is_empty(),
        "level {} is already built",
        top - 1
    );
}

#[test]
fn a_prefetch_outlived_by_its_image_does_nothing() {
    let pyramid = pyramid_of(&noise(4300, 16, 6));
    let builds = pyramid.prefetch_around(1);
    drop(pyramid);
    builds.into_iter().for_each(|b| b.join().unwrap());
}

// ── Wire format ───────────────────────────────────────────────────────────────

#[test]
fn the_tile_payload_carries_each_tile_behind_its_header() {
    let pyramid = pyramid_of(&noise(1100, 600, 8));
    let tiles = vec![
        pyramid.tile(0, 2, 1).unwrap(),
        pyramid.tile(0, 0, 0).unwrap(),
    ];
    let payload = build_tile_payload(&tiles);

    let u32_at = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
    assert_eq!(u32_at(0), 2);

    let mut at = 4;
    for tile in &tiles {
        assert_eq!(
            [u32_at(at), u32_at(at + 4), u32_at(at + 8), u32_at(at + 12)],
            [tile.column, tile.row, tile.width, tile.height]
        );
        at += TILE_HEADER_BYTES;
        assert_eq!(&payload[at..at + tile.pixels.len()], tile.pixels.as_slice());
        at += tile.pixels.len();
    }
    assert_eq!(at, payload.len());
}
