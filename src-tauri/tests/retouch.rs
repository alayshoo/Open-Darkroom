// src-tauri/tests/retouch.rs
//
// Heal and clone strokes: their geometry, retouch.wgsl applying them to the
// linear source, and the export rendering them the same however it is chunked.

mod common;

use common::{device, noise};

use image::{ImageBuffer, Rgba};
use open_darkroom_lib::crop::CropPayload;
use open_darkroom_lib::export_rendering::{render_cropped_with_chunk_rows, SlidersPayload};
use open_darkroom_lib::icc::SourceSpace;
use open_darkroom_lib::image_opening::Rgba16Image;
use open_darkroom_lib::output_space::OutputSpace;
use open_darkroom_lib::retouch::{
    apply_retouch, blur_span, place_retouch, retouch_tiles, retouch_uniform, LinearSource, RetouchMode,
    RetouchPayload, RETOUCH_TILE, RETOUCH_UNIFORM_BYTES,
};

fn identity_luts() -> [Vec<f32>; 3] {
    let lut: Vec<f32> = (0..=u16::MAX as u32).map(|i| i as f32 / u16::MAX as f32).collect();
    [lut.clone(), lut.clone(), lut]
}

fn stroke(mode: RetouchMode, points: &[[f64; 2]], radius: f64, offset: [f64; 2]) -> RetouchPayload {
    RetouchPayload { mode, points: points.to_vec(), radius, feather: 0.0, opacity: 1.0, offset }
}

fn texel(bytes: &[u8], width: u32, x: u32, y: u32) -> [f32; 4] {
    let at = ((y * width + x) * 16) as usize;
    let mut out = [0.0; 4];
    for (c, v) in out.iter_mut().enumerate() {
        *v = f32::from_le_bytes(bytes[at + c * 4..at + c * 4 + 4].try_into().unwrap());
    }
    out
}

/// `img` retouched by `strokes`, read back whole.
fn retouched(img: &Rgba16Image, strokes: &[RetouchPayload]) -> Vec<u8> {
    let (w, h) = img.dimensions();
    let luts = identity_luts();
    let (device, queue) = device();
    let mut source = LinearSource::new(img.as_raw(), w, h, 4, &luts);
    apply_retouch(device, queue, &mut source, strokes).expect("retouch");
    source.read(0, 0, w, h)
}

fn grey(width: u32, height: u32, v: u16) -> Rgba16Image {
    ImageBuffer::from_pixel(width, height, Rgba([v, v, v, u16::MAX]))
}

// ── Geometry ──────────────────────────────────────────────────────────────────

#[test]
fn offsets_round_halves_up_as_the_frontend_does() {
    let r = stroke(RetouchMode::Clone, &[[0.5, 0.5]], 0.01, [-2.5 / 100.0, 2.5 / 100.0]);
    assert_eq!(place_retouch(&r, 100, 100).offset, [-2, 3]);
}

#[test]
fn a_long_stroke_is_cut_into_tiles_that_skip_where_it_does_not_reach() {
    // A diagonal across a 2048 square: only the tiles along the diagonal.
    let r = stroke(RetouchMode::Clone, &[[0.0, 0.0], [1.0, 1.0]], 4.0 / 2048.0, [0.0, 0.0]);
    let tiles = retouch_tiles(&place_retouch(&r, 2048, 2048), 2048, 2048);
    let per_side = (2048 / RETOUCH_TILE) as usize;
    assert!(tiles.len() < per_side * per_side);
    assert!(tiles.len() >= per_side);
    for t in &tiles {
        assert!(t.target[2] - t.target[0] <= RETOUCH_TILE);
        assert!(!t.segments.is_empty());
    }
}

#[test]
fn a_heal_reads_its_blur_reach_past_each_tile() {
    let r = stroke(RetouchMode::Heal, &[[0.5, 0.5]], 10.0 / 1000.0, [0.1, 0.0]);
    let placed = place_retouch(&r, 1000, 1000);
    let reach = blur_span(placed.radius).reach;
    let tile = &retouch_tiles(&placed, 1000, 1000)[0];
    assert_eq!(tile.target[0] - tile.work[0], reach);
}

#[test]
fn the_uniform_is_laid_out_as_retouch_wgsl_declares_it() {
    let r = stroke(RetouchMode::Heal, &[[0.5, 0.5]], 0.01, [0.1, -0.1]);
    let placed = place_retouch(&r, 1000, 500);
    let tile = &retouch_tiles(&placed, 1000, 500)[0];
    let bytes = retouch_uniform(&placed, tile, [7, 9], 1000, 500);
    let f = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let u = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!(bytes.len(), RETOUCH_UNIFORM_BYTES);
    assert_eq!([f(6), f(7)], [7.0, 9.0]);
    assert_eq!([f(8), f(9)], [1000.0, 500.0]);
    assert_eq!([f(10), f(11)], [100.0, -50.0]);
    assert_eq!(f(12), 10.0);
    assert_eq!(u(18), 1);
    assert_eq!(u(19), 1);
}

// ── retouch.wgsl ──────────────────────────────────────────────────────────────

#[test]
fn a_clone_copies_its_source_exactly_and_leaves_the_rest_alone() {
    let img = noise(96, 64, 7);
    let r = stroke(RetouchMode::Clone, &[[0.75, 0.5]], 6.0 / 96.0, [-48.0 / 96.0, 0.0]);
    let before = retouched(&img, &[]);
    let after = retouched(&img, &[r]);
    // The centre takes the texel 48 to the left of it, untouched.
    assert_eq!(texel(&after, 96, 72, 32), texel(&before, 96, 24, 32));
    // Well clear of the stroke nothing moves.
    assert_eq!(texel(&after, 96, 72, 50), texel(&before, 96, 72, 50));
    assert_eq!(texel(&after, 96, 10, 10), texel(&before, 96, 10, 10));
}

#[test]
fn a_heal_takes_the_tone_from_around_the_stroke() {
    // A bright speck on mid grey, healed from a darker grey patch.
    let mut img = grey(128, 64, 0x6000);
    for y in 0..64 {
        for x in 0..48 {
            img.put_pixel(x, y, Rgba([0x2000, 0x2000, 0x2000, u16::MAX]));
        }
    }
    for y in 28..36 {
        for x in 92..100 {
            img.put_pixel(x, y, Rgba([0xF000, 0xF000, 0xF000, u16::MAX]));
        }
    }
    let r = stroke(RetouchMode::Heal, &[[96.0 / 128.0, 32.0 / 64.0]], 8.0 / 128.0, [-72.0 / 128.0, 0.0]);
    let after = retouched(&img, &[r]);
    let expected = 0x6000 as f32 / u16::MAX as f32;
    for (x, y) in [(96, 32), (93, 29), (99, 35)] {
        let [r, g, b, a] = texel(&after, 128, x, y);
        for v in [r, g, b] {
            assert!((v - expected).abs() < 2e-3, "({x}, {y}) is {v}, expected {expected}");
        }
        assert_eq!(a, 1.0);
    }
}

#[test]
fn a_heal_across_tiles_leaves_no_seam() {
    // A dark hair across more than two tiles of flat grey.
    let width = 2 * RETOUCH_TILE as u32 + 200;
    let mut img = grey(width, 96, 0x8000);
    for x in 4..width - 4 {
        for y in 46..49 {
            img.put_pixel(x, y, Rgba([0x1000, 0x1000, 0x1000, u16::MAX]));
        }
    }
    let r = stroke(
        RetouchMode::Heal,
        &[[4.0 / width as f64, 47.5 / 96.0], [(width - 4) as f64 / width as f64, 47.5 / 96.0]],
        4.0 / width as f64,
        [0.0, -20.0 / 96.0],
    );
    let after = retouched(&img, &[r]);
    let expected = 0x8000 as f32 / u16::MAX as f32;
    for x in (8..width - 8).step_by(7) {
        for y in 44..52 {
            let v = texel(&after, width, x, y)[0];
            assert!((v - expected).abs() < 2e-3, "({x}, {y}) is {v}");
        }
    }
}

#[test]
fn later_strokes_see_earlier_ones() {
    // The second clone takes its pixels from where the first wrote.
    let img = noise(96, 32, 3);
    let first = stroke(RetouchMode::Clone, &[[48.0 / 96.0, 16.0 / 32.0]], 4.0 / 96.0, [-40.0 / 96.0, 0.0]);
    let second = stroke(RetouchMode::Clone, &[[88.0 / 96.0, 16.0 / 32.0]], 2.0 / 96.0, [-40.0 / 96.0, 0.0]);
    let before = retouched(&img, &[]);
    let after = retouched(&img, &[first, second]);
    assert_eq!(texel(&after, 96, 88, 16), texel(&before, 96, 8, 16));
}

#[test]
fn the_preview_source_search_compiles() {
    let (device, _) = device();
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("retouchSource"),
        source: wgpu::ShaderSource::Wgsl(open_darkroom_lib::shaders::RETOUCH_SOURCE_WGSL.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let error = pollster::block_on(device.pop_error_scope());
    assert!(error.is_none(), "retouchSource.wgsl: {error:?}");
}

// ── Export ────────────────────────────────────────────────────────────────────

#[test]
fn a_retouched_export_is_the_same_however_it_is_chunked() {
    let img = noise(160, 120, 11);
    let strokes = [
        stroke(RetouchMode::Heal, &[[0.2, 0.2], [0.8, 0.9]], 6.0 / 160.0, [0.25, -0.1]),
        stroke(RetouchMode::Clone, &[[0.5, 0.95]], 5.0 / 160.0, [0.0, -0.8]),
    ];
    let render = |chunk_rows| {
        pollster::block_on(render_cropped_with_chunk_rows(
            img.as_raw(),
            160,
            120,
            4,
            &SlidersPayload::default(),
            &CropPayload::FULL_FRAME,
            &strokes,
            &SourceSpace::default(),
            OutputSpace::Srgb,
            8,
            chunk_rows,
            |_| {},
        ))
        .expect("render")
        .rgb
    };
    let plain = pollster::block_on(render_cropped_with_chunk_rows(
        img.as_raw(),
        160,
        120,
        4,
        &SlidersPayload::default(),
        &CropPayload::FULL_FRAME,
        &[],
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        120,
        |_| {},
    ))
    .expect("render")
    .rgb;
    let whole = render(120);
    assert_ne!(whole, plain, "the strokes changed nothing");
    assert_eq!(render(17), whole);
}
