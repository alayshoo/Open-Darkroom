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
use open_darkroom_lib::crop::crop_view;
use open_darkroom_lib::image_opening::{
    compute_histograms_in, compute_retouched_histograms_in, Histograms, Rgba16Image, RetouchedHistograms, MAX_RETOUCHED_BYTES,
};
use open_darkroom_lib::output_space::OutputSpace;
use open_darkroom_lib::retouch::{
    apply_retouch, blur_span, follow_strokes, place_retouch, retouch_tiles, retouch_uniform, Followed, LinearSource,
    RetouchMode, RetouchPasses, RetouchPayload, StrokeJournal, RETOUCH_TILE, RETOUCH_UNIFORM_BYTES,
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
    let bytes = retouch_uniform(&placed, tile, [7, 9], [-3, 4], 1000, 500);
    let f = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let u = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    assert_eq!(bytes.len(), RETOUCH_UNIFORM_BYTES);
    assert_eq!([f(6), f(7)], [7.0, 9.0]);
    assert_eq!([f(8), f(9)], [1000.0, 500.0]);
    assert_eq!([f(10), f(11)], [100.0, -50.0]);
    assert_eq!(f(12), 10.0);
    assert_eq!(u(18), 1);
    assert_eq!(u(19), 1);
    assert_eq!([f(20), f(21)], [-3.0, 4.0]);
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
fn a_source_further_away_than_a_texture_reaches_is_still_copied() {
    // Wider than wgpu's default 8192 texel limit between the stroke and its source.
    let img = noise(9000, 16, 3);
    let r = stroke(RetouchMode::Clone, &[[8950.0 / 9000.0, 0.5]], 4.0 / 9000.0, [-8900.0 / 9000.0, 0.0]);
    let before = retouched(&img, &[]);
    let after = retouched(&img, &[r]);
    assert_eq!(texel(&after, 9000, 8950, 8), texel(&before, 9000, 50, 8));
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
    let _ = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
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

// ── Histograms ────────────────────────────────────────────────────────────────

#[test]
fn the_input_histograms_count_a_healed_speck_as_what_replaced_it() {
    // Mid grey with a bright speck, healed from more of the same grey.
    let mut img = grey(128, 64, 0x6000);
    for y in 30..34 {
        for x in 94..98 {
            img.put_pixel(x, y, Rgba([0xF000, 0xF000, 0xF000, u16::MAX]));
        }
    }
    let bins: Vec<u8> = (0..=u16::MAX as u32).map(|i| (i >> 8) as u8).collect();
    let view = crop_view(&CropPayload::FULL_FRAME, 128, 64);
    let luts = identity_luts();
    let (device, queue) = device();
    let r = stroke(RetouchMode::Heal, &[[96.0 / 128.0, 32.0 / 64.0]], 6.0 / 128.0, [-40.0 / 128.0, 0.0]);

    let before = compute_histograms_in(img.as_raw(), 128, 64, 4, &view, &bins);
    let after =
        compute_retouched_histograms_in(device, queue, img.as_raw(), 128, 64, 4, &view, &bins, &luts, &[r]).unwrap();
    assert_eq!(before.r[0xF0], 16);
    assert_eq!(after.r[0xF0], 0);
    assert_eq!(after.r.iter().sum::<u32>(), 128 * 64);
    assert_eq!(after.r[0x60] - before.r[0x60], 16);
}

/// Strokes spread over a noisy image, each on its own spot.
fn spots(width: u32, height: u32) -> Vec<RetouchPayload> {
    let r = 5.0 / width.max(height) as f64;
    [(0.2, 0.3), (0.5, 0.5), (0.8, 0.7), (0.5, 0.52), (0.3, 0.8)]
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let mode = if i % 2 == 0 { RetouchMode::Heal } else { RetouchMode::Clone };
            stroke(mode, &[[x, y], [x + 0.05, y + 0.02]], r, [-0.15, -0.1])
        })
        .collect()
}

/// Follows `steps` in turn with one journal, checking each against the same strokes applied afresh.
fn follow_through(img: &Rgba16Image, cap: usize, steps: &[Vec<RetouchPayload>]) -> Vec<Followed> {
    let (w, h) = img.dimensions();
    let luts = identity_luts();
    let (device, queue) = device();
    let passes = RetouchPasses::new(device);
    let mut journal = StrokeJournal::with_cap(cap);
    let mut blocks = Default::default();
    let mut outcomes = Vec::new();
    for strokes in steps {
        let mut source = LinearSource::with_blocks(img.as_raw(), w, h, 4, &luts, blocks);
        outcomes.push(follow_strokes(device, queue, &passes, &mut source, &mut journal, strokes, |_, _| {}).unwrap());
        assert_eq!(source.read(0, 0, w, h), retouched(img, strokes), "after following {} strokes", strokes.len());
        assert_eq!(journal.strokes(), strokes.as_slice());
        blocks = source.into_blocks();
    }
    outcomes
}

#[test]
fn strokes_rolled_back_leave_the_image_as_if_never_applied() {
    let img = noise(160, 120, 5);
    let all = spots(160, 120);
    let steps = vec![
        all.clone(),
        all[..2].to_vec(),
        vec![all[0].clone(), all[3].clone()],
        vec![],
        all[..4].to_vec(),
    ];
    assert!(follow_through(&img, usize::MAX, &steps).iter().all(|f| *f == Followed::Stepped));
}

#[test]
fn past_the_cap_the_oldest_strokes_are_reapplied_from_the_untouched_image() {
    let img = noise(160, 120, 6);
    let all = spots(160, 120);
    // Room for about one stroke's blocks, so only the newest can be rolled back.
    let outcomes = follow_through(&img, 300_000, &[all.clone(), all[..4].to_vec(), all[..1].to_vec()]);
    assert_eq!(outcomes, [Followed::Stepped, Followed::Stepped, Followed::Restarted]);
}

fn same_counts(a: &Histograms, b: &Histograms) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b
}

/// Follows strokes and crops with one [`RetouchedHistograms`], checking each
/// step against binning afresh, and returns the bytes held after each.
fn follow_histograms(budget: usize) -> Vec<usize> {
    let (w, h) = (160, 120);
    let img = noise(w, h, 9);
    let bins: Vec<u8> = (0..=u16::MAX as u32).map(|i| (i >> 8) as u8).collect();
    let luts = identity_luts();
    let (device, queue) = device();
    let passes = RetouchPasses::new(device);
    let tight = CropPayload { cx: 0.45, cy: 0.55, w: 0.6, h: 0.5, angle: 0.2, ..CropPayload::FULL_FRAME };
    let all = spots(w, h);
    let steps = [
        (CropPayload::FULL_FRAME, all[..3].to_vec()),
        (CropPayload::FULL_FRAME, all.clone()),
        (CropPayload::FULL_FRAME, all[..1].to_vec()),
        (tight, all[..1].to_vec()),
        (tight, vec![all[0].clone(), all[4].clone()]),
        (tight, all[..4].to_vec()),
    ];
    let mut kept = RetouchedHistograms::with_budget(budget);
    let mut held = Vec::new();
    for (crop, strokes) in &steps {
        let got = kept
            .update(device, queue, &passes, img.as_raw(), w, h, 4, crop, &bins, &luts, strokes)
            .unwrap();
        let view = crop_view(crop, w, h);
        let fresh =
            compute_retouched_histograms_in(device, queue, img.as_raw(), w, h, 4, &view, &bins, &luts, strokes).unwrap();
        assert!(same_counts(&got, &fresh), "{} strokes in {crop:?}", strokes.len());
        held.push(kept.held_bytes());
    }
    held
}

#[test]
fn the_kept_histograms_match_binning_afresh_as_strokes_and_crops_change() {
    assert!(follow_histograms(MAX_RETOUCHED_BYTES).iter().all(|&bytes| bytes > 0));
}

#[test]
fn past_the_budget_nothing_is_kept_and_the_histograms_still_match() {
    assert!(follow_histograms(1).iter().all(|&bytes| bytes == 0));
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
