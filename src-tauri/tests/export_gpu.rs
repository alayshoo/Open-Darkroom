// src-tauri/tests/export_gpu.rs
//
// Layer 2: the mechanics of the export renderer rather than the colour maths —
// chunking, row padding, bit depth, progress and input validation. These are
// the parts that break on unusual image dimensions rather than unusual sliders.

mod common;

use common::{
    apply_matrix, linear_to_srgb, noise, rgb_ramp, solid, srgb_to_linear, u8_to_u16,
    WORKING_LUMA, WORKING_TO_SRGB,
};

use open_darkroom_lib::export_rendering::{
    render_image, render_image_with_chunk_rows, SlidersPayload, CHUNK_ROWS,
};
use open_darkroom_lib::output_space::OutputSpace;
use open_darkroom_lib::icc::{SourceSpace, IDENTITY};
use open_darkroom_lib::image_opening::Rgba16Image;

fn render8(img: &Rgba16Image, sliders: &SlidersPayload) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_image(img.as_raw(), w, h, 4, sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {})).expect("render")
}

fn render_chunked(img: &Rgba16Image, chunk_rows: u32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        w,
        h,
        4,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        chunk_rows,
        |_| {},
    ))
    .expect("render")
}

// ── Chunking ──────────────────────────────────────────────────────────────────

#[test]
fn chunk_size_does_not_change_the_result() {
    let img = rgb_ramp(32, 40);

    let whole = render_chunked(&img, 40);
    for chunk_rows in [1, 3, 7, 8, 39, 41, 1000] {
        assert_eq!(
            render_chunked(&img, chunk_rows),
            whole,
            "chunking at {chunk_rows} rows changed the output"
        );
    }
}

#[test]
fn no_seam_appears_at_a_chunk_boundary() {
    // A vertical gradient crossing several chunk boundaries. Each row must
    // differ from the last by roughly the same amount; a seam shows up as a
    // step at a multiple of the chunk height.
    let height = 64u32;
    let img = Rgba16Image::from_fn(8, height, |_, y| {
        let v = (y * 255 / (height - 1)) as u8;
        image::Rgba([u8_to_u16(v), u8_to_u16(v), u8_to_u16(v), u16::MAX])
    });

    let out = render_chunked(&img, 8);

    let mut deltas = Vec::new();
    for y in 1..height as usize {
        let prev = out[(y - 1) * 8 * 3] as i32;
        let curr = out[y * 8 * 3] as i32;
        deltas.push(curr - prev);
    }

    let max = *deltas.iter().max().unwrap();
    let min = *deltas.iter().min().unwrap();
    assert!(
        max - min <= 2,
        "row-to-row steps ranged {min}..{max} — a chunk boundary left a seam"
    );
}

#[test]
fn images_shorter_than_one_chunk_render_whole() {
    let img = rgb_ramp(16, 3);
    assert_eq!(render_chunked(&img, CHUNK_ROWS).len(), 16 * 3 * 3);
}

// ── Row padding and odd dimensions ────────────────────────────────────────────

#[test]
fn awkward_widths_survive_row_padding() {
    // Readback rows are padded to 256 bytes. At 4 bytes per texel any width
    // that is not a multiple of 64 exercises the unpadding path.
    for width in [1u32, 3, 37, 64, 65, 101, 127, 128, 255, 257] {
        let img = rgb_ramp(width, 5);
        let out = render8(&img, &SlidersPayload::default());

        assert_eq!(
            out.len(),
            (width * 5 * 3) as usize,
            "width {width}: wrong output length"
        );

        // Row 0 and row 4 hold identical source data, so they must render the
        // same. A padding error shifts later rows sideways.
        let row = |y: usize| &out[y * width as usize * 3..(y + 1) * width as usize * 3];
        assert_eq!(row(0), row(4), "width {width}: rows diverged");
    }
}

#[test]
fn awkward_dimensions_survive_at_16_bit() {
    // 16 bytes per texel, so the padding boundary falls at different widths.
    for width in [1u32, 3, 15, 16, 17, 31, 33, 97] {
        let img = rgb_ramp(width, 3);
        let out = pollster::block_on(render_image(
            img.as_raw(),
            width,
            3,
            4,
            &SlidersPayload::default(),
            &SourceSpace::default(),
            OutputSpace::Srgb,
            16,
            |_| {},
        ))
        .expect("render");

        assert_eq!(out.len(), (width * 3 * 6) as usize, "width {width}");
    }
}

#[test]
fn a_single_pixel_renders() {
    let img = solid(1, 1, [u8_to_u16(128); 4]);
    let out = render8(&img, &SlidersPayload::default());

    assert_eq!(out.len(), 3);
    for (c, &v) in out.iter().enumerate() {
        assert!((v as i32 - 128).abs() <= 2, "channel {c} gave {v}");
    }
}

// ── Bit depth ─────────────────────────────────────────────────────────────────

#[test]
fn eight_and_sixteen_bit_paths_agree() {
    let img = noise(32, 16, 0xBEEF);
    let sliders = SlidersPayload::default();

    let eight = render8(&img, &sliders);
    let sixteen = pollster::block_on(render_image(img.as_raw(), 32, 16, 4, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 16, |_| {}))
        .expect("render");

    assert_eq!(eight.len(), 32 * 16 * 3);
    assert_eq!(sixteen.len(), 32 * 16 * 6);

    for i in 0..(32 * 16 * 3) {
        let wide = u16::from_le_bytes([sixteen[i * 2], sixteen[i * 2 + 1]]);
        let narrow = eight[i] as i32;
        let reduced = (wide >> 8) as i32;
        assert!(
            (narrow - reduced).abs() <= 2,
            "sample {i}: 8-bit gave {narrow}, 16-bit reduced to {reduced}"
        );
    }
}

#[test]
fn sixteen_bit_output_carries_more_levels_than_eight() {
    // A shallow gradient that 8-bit quantises but 16-bit should resolve.
    let img = Rgba16Image::from_fn(256, 1, |x, _| {
        let v = 1000 + x as u16 * 4;
        image::Rgba([v, v, v, u16::MAX])
    });

    let sixteen = pollster::block_on(render_image(
        img.as_raw(),
        256,
        1,
        4,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        16,
        |_| {},
    ))
    .expect("render");

    let values: std::collections::HashSet<u16> = sixteen
        .chunks_exact(6)
        .map(|px| u16::from_le_bytes([px[0], px[1]]))
        .collect();

    let eight = render8(&img, &SlidersPayload::default());
    let narrow: std::collections::HashSet<u8> = eight.chunks_exact(3).map(|px| px[0]).collect();

    assert!(
        values.len() > narrow.len(),
        "16-bit resolved {} levels, 8-bit resolved {} — no benefit",
        values.len(),
        narrow.len()
    );
}

/// The 16-bit path end to end, on the ramp that used to expose its ceiling.
///
/// Above linear 0.5 an f16 texture steps in 4.9e-4 while one 16-bit sRGB code is
/// 3.5e-5 apart, so an f16 upload or render target collapses runs of about
/// seventeen adjacent codes onto one value — roughly 11 real bits in a 16-bit
/// container, and visible banding on any smooth highlight. Both surfaces are
/// `rgba32float`, so the ramp has to survive code for code.
#[test]
fn a_highlight_ramp_keeps_every_sixteen_bit_code() {
    // 256 consecutive codes just below white, where the collapse was worst.
    let img = Rgba16Image::from_fn(256, 1, |x, _| {
        let v = 65_280 + x as u16;
        image::Rgba([v, v, v, u16::MAX])
    });

    let out = pollster::block_on(render_image(
        img.as_raw(),
        256,
        1,
        4,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        16,
        |_| {},
    ))
    .expect("render");

    let levels: Vec<u16> = out
        .chunks_exact(6)
        .map(|px| u16::from_le_bytes([px[0], px[1]]))
        .collect();

    // Neutral sliders are an identity, so each output code must track its input.
    for (x, &got) in levels.iter().enumerate() {
        let expected = 65_280 + x as i32;
        assert!(
            (got as i32 - expected).abs() <= 4,
            "code {expected} came back as {got}"
        );
    }

    let distinct: std::collections::HashSet<u16> = levels.iter().copied().collect();
    assert!(
        distinct.len() >= 200,
        "256 consecutive input codes resolved to {} output levels — the ramp is \
         being quantised somewhere in the chain",
        distinct.len()
    );

    // A collapse shows up as a run of identical codes followed by a jump; the
    // step between neighbours is what catches it.
    let worst = levels
        .windows(2)
        .map(|w| (w[1] as i32 - w[0] as i32).abs())
        .max()
        .unwrap();
    assert!(worst <= 4, "adjacent codes stepped by {worst}, not by 1");
}

// ── Progress reporting ────────────────────────────────────────────────────────

#[test]
fn progress_increases_and_finishes_at_one() {
    use std::sync::Mutex;

    let img = rgb_ramp(16, 30);
    let seen = Mutex::new(Vec::new());

    pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        16,
        30,
        4,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        10,
        |p| seen.lock().unwrap().push(p),
    ))
    .expect("render");

    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3, "30 rows in chunks of 10");
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "must increase: {seen:?}");
    assert_eq!(*seen.last().unwrap(), 1.0, "must finish at 100%");
    assert!(seen.iter().all(|&p| (0.0..=1.0).contains(&p)), "{seen:?}");
}

#[test]
fn a_ragged_final_chunk_still_reports_completion() {
    use std::sync::Mutex;

    let img = rgb_ramp(8, 25);
    let seen = Mutex::new(Vec::new());

    pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        8,
        25,
        4,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        10,
        |p| seen.lock().unwrap().push(p),
    ))
    .expect("render");

    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 3, "25 rows in chunks of 10 leaves a 5-row tail");
    assert_eq!(*seen.last().unwrap(), 1.0);
}

// ── Input validation ──────────────────────────────────────────────────────────

#[test]
fn malformed_input_is_rejected_before_touching_the_gpu() {
    let sliders = SlidersPayload::default();

    let err = pollster::block_on(render_image(&[0u16; 16], 4, 4, 4, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {}))
        .expect_err("buffer too small for 4×4 RGBA");
    assert!(err.contains("expected 64"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[], 0, 4, 4, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {}))
        .expect_err("zero width");
    assert!(err.contains("0 × 4"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[], 4, 0, 4, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {}))
        .expect_err("zero height");
    assert!(err.contains("4 × 0"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image_with_chunk_rows(
        &[0u16; 16],
        2,
        2,
        4,
        &sliders,
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        0,
        |_| {},
    ))
    .expect_err("zero chunk size");
    assert!(err.contains("chunk_rows"), "unhelpful error: {err}");

    // A three-channel buffer sized as if it were four, and an unsupported
    // channel count — the stride is what the slicing arithmetic trusts.
    let err = pollster::block_on(render_image(&[0u16; 64], 4, 4, 3, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {}))
        .expect_err("buffer sized for RGBA, declared RGB");
    assert!(err.contains("expected 48"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[0u16; 32], 4, 4, 2, &sliders, &SourceSpace::default(), OutputSpace::Srgb, 8, |_| {}))
        .expect_err("two channels is not a thing");
    assert!(err.contains("channels must be"), "unhelpful error: {err}");
}

// ── Channel count ─────────────────────────────────────────────────────────────

/// Sources with no alpha channel are stored three-wide, so the renderer has to
/// slice them at a stride of three and synthesise the alpha on upload. Since the
/// develop chain never reads alpha and the output is RGB, the two layouts must
/// produce byte-identical files.
#[test]
fn a_three_channel_source_renders_exactly_like_its_rgba_equivalent() {
    let rgba = noise(24, 18, 0x5EED);

    let rgb: Vec<u16> = rgba
        .as_raw()
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect();

    for bit_depth in [8u8, 16] {
        let from_rgba = pollster::block_on(render_image(
            rgba.as_raw(),
            24,
            18,
            4,
            &SlidersPayload::default(),
            &SourceSpace::default(),
            OutputSpace::Srgb,
bit_depth,
            |_| {},
        ))
        .expect("render");

        let from_rgb = pollster::block_on(render_image(
            &rgb,
            24,
            18,
            3,
            &SlidersPayload::default(),
            &SourceSpace::default(),
            OutputSpace::Srgb,
bit_depth,
            |_| {},
        ))
        .expect("render");

        assert_eq!(
            from_rgb, from_rgba,
            "dropping the alpha channel changed the {bit_depth}-bit render"
        );
    }
}

/// The chunked path slices the source buffer itself, so a stride error would
/// show up as a seam rather than as a uniformly wrong image.
#[test]
fn chunking_a_three_channel_source_leaves_no_seam() {
    let rgba = rgb_ramp(16, 40);
    let rgb: Vec<u16> = rgba
        .as_raw()
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2]])
        .collect();

    let whole = pollster::block_on(render_image_with_chunk_rows(
        &rgb,
        16,
        40,
        3,
        &SlidersPayload::default(),
        &SourceSpace::default(),
        OutputSpace::Srgb,
        8,
        40,
        |_| {},
    ))
    .expect("render");

    for chunk_rows in [1, 7, 13, 39, 41] {
        let chunked = pollster::block_on(render_image_with_chunk_rows(
            &rgb,
            16,
            40,
            3,
            &SlidersPayload::default(),
            &SourceSpace::default(),
            OutputSpace::Srgb,
            8,
            chunk_rows,
            |_| {},
        ))
        .expect("render");

        assert_eq!(
            chunked, whole,
            "chunking a three-channel source at {chunk_rows} rows changed the output"
        );
    }
}

// ── The output transform ──────────────────────────────────────────────────────

/// The chain now works in a space wider than the one it renders to, so it ends
/// in a real gamut conversion where it used to end in the identity. With neutral
/// sliders the whole round trip — sRGB in, into linear Rec.2020, back out to
/// sRGB — has to return the file unchanged.
///
/// Saturated primaries are what make this worth asserting. A transposed or
/// reversed matrix still maps a neutral to a neutral, so a grey ramp passes
/// either way; pure red, green and blue are where the two come apart. Cyan is
/// there because it is the corner sRGB and Rec.2020 disagree about most.
#[test]
fn a_neutral_render_round_trips_through_the_working_space() {
    const PROBES: [[u8; 3]; 8] = [
        [0, 0, 0],
        [255, 255, 255],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [0, 255, 255],
        [200, 120, 40],
    ];

    let img: Rgba16Image = image::ImageBuffer::from_fn(PROBES.len() as u32, 1, |x, _| {
        let p = PROBES[x as usize];
        image::Rgba([
            p[0] as u16 * 257,
            p[1] as u16 * 257,
            p[2] as u16 * 257,
            u16::MAX,
        ])
    });

    let out = render8(&img, &SlidersPayload::default());

    for (i, want) in PROBES.iter().enumerate() {
        let got = [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]];
        for c in 0..3 {
            let delta = got[c] as i32 - want[c] as i32;
            assert!(
                delta.abs() <= 2,
                "probe {i} channel {c}: expected {} ± 2, got {} (whole pixel {got:?}, \
                 wanted {want:?}) — the source → working matrix and the output \
                 transform are not inverses",
                want[c],
                got[c]
            );
        }
    }
}

/// Every other export test renders through `SourceSpace::default()`, where the
/// source matrix and the output transform are inverses and the round trip hides
/// whether the uniform was read at all. A buffer never bound, bound at the
/// wrong index, or packed without the column padding would leave the matrix at
/// zero or garbage, and the neutral round trip would be the first thing to go
/// quiet about it.
///
/// So this renders a file that declares itself already in the working space.
/// The source matrix is then the identity and only the output transform runs,
/// which is a conversion with a closed form the export has to match.
#[test]
fn the_export_applies_the_source_matrix_it_is_given() {
    let probes: [[u8; 3]; 4] = [[180, 120, 60], [120, 60, 180], [90, 140, 110], [128, 128, 128]];

    let img: Rgba16Image = image::ImageBuffer::from_fn(probes.len() as u32, 1, |x, _| {
        let p = probes[x as usize];
        image::Rgba([u8_to_u16(p[0]), u8_to_u16(p[1]), u8_to_u16(p[2]), u16::MAX])
    });

    let mut already_working = SourceSpace::default();
    already_working.to_working = IDENTITY;

    let (w, h) = img.dimensions();
    let out = pollster::block_on(render_image(
        img.as_raw(),
        w,
        h,
        4,
        &SlidersPayload::default(),
        &already_working,
        OutputSpace::Srgb,
        8,
        |_| {},
    ))
    .expect("render");

    for (i, probe) in probes.iter().enumerate() {
        let linear: [f64; 3] = std::array::from_fn(|c| srgb_to_linear(probe[c] as f64 / 255.0));
        let dst = apply_matrix(&WORKING_TO_SRGB, linear);
        let want: [u8; 3] =
            std::array::from_fn(|c| (linear_to_srgb(dst[c].clamp(0.0, 1.0)) * 255.0).round() as u8);

        for c in 0..3 {
            let got = out[i * 3 + c];
            assert!(
                (got as i32 - want[c] as i32).abs() <= 2,
                "probe {i} channel {c}: expected {} +/- 2, got {got} (whole pixel                  {:?}, wanted {want:?}) - the identity source matrix was not the                  matrix the export used",
                want[c],
                [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]]
            );
        }
    }

    // A neutral is the one probe both matrices agree on, so if it alone passed
    // the matrix could still be anything. Say so rather than relying on order.
    let grey = [out[9], out[10], out[11]];
    assert!(
        grey.iter().all(|&c| (c as i32 - 128).abs() <= 2),
        "the neutral probe should survive either way, got {grey:?}"
    );
}


// ── The destination ───────────────────────────────────────────────────────────

/// Rec.709 luminance weights — the Y row of linear sRGB → XYZ.
const SRGB_LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// How far the rendered luminance may sit from the input's.
///
/// The compression preserves it exactly, so this is the 8-bit quantisation of
/// the result and nothing else — worth about 0.0015 at the steepest point of
/// the curve, against a measured worst case of 0.0005.
const LUMA_TOLERANCE: f64 = 0.005;

/// Render probes that are already in the working space, so only the output
/// transform runs and its closed form is what the result has to match.
fn render_to(probes: &[[u8; 3]], output: OutputSpace) -> Vec<u8> {
    let img: Rgba16Image = image::ImageBuffer::from_fn(probes.len() as u32, 1, |x, _| {
        let p = probes[x as usize];
        image::Rgba([u8_to_u16(p[0]), u8_to_u16(p[1]), u8_to_u16(p[2]), u16::MAX])
    });

    let mut already_working = SourceSpace::default();
    already_working.to_working = IDENTITY;

    let (w, h) = img.dimensions();
    pollster::block_on(render_image(
        img.as_raw(),
        w,
        h,
        4,
        &SlidersPayload::default(),
        &already_working,
        output,
        8,
        |_| {},
    ))
    .expect("render")
}

fn pixel(out: &[u8], i: usize) -> [u8; 3] {
    [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]]
}

/// What the gamut compression promises, stated as the one thing that is exactly
/// true of it: luminance survives.
///
/// Colour that leaves the destination's gamut is pulled towards the neutral of
/// its own brightness, so it loses saturation and keeps its hue and its
/// luminance. A per-channel clamp keeps neither — it drops whichever channel
/// went out of range and leaves the others, which both darkens the colour and
/// slides its hue.
///
/// The Rec.2020 primaries are the sharpest case: all three are far outside sRGB,
/// so the two behaviours are tens of code values apart rather than one or two.
#[test]
fn the_gamut_compression_keeps_the_luminance_a_clamp_would_lose() {
    // Full-intensity working-space primaries, plus a neutral that needs no
    // compression at all and must therefore come through either way.
    let probes: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128]];
    let out = render_to(&probes, OutputSpace::Srgb);

    for (i, probe) in probes.iter().enumerate() {
        let working: [f64; 3] = std::array::from_fn(|c| srgb_to_linear(probe[c] as f64 / 255.0));
        let want: f64 = (0..3).map(|c| WORKING_LUMA[c] * working[c]).sum();

        let got_rgb = pixel(&out, i);
        let linear: [f64; 3] = std::array::from_fn(|c| srgb_to_linear(got_rgb[c] as f64 / 255.0));
        let got: f64 = (0..3).map(|c| SRGB_LUMA[c] * linear[c]).sum();

        assert!(
            (got - want).abs() < LUMA_TOLERANCE,
            "probe {probe:?} → {got_rgb:?}: luminance is {got:.4}, expected {want:.4} — \
             the compression is not aiming at the neutral of the pixel's own brightness"
        );

        // And prove the assertion above is not vacuous: the clamp this replaced
        // would have failed it. The margin is widest on the primaries the two
        // spaces disagree about most — 0.050 of luminance on red, 0.037 on
        // green, 0.013 on blue — and the neutral is in gamut, so it is the one
        // probe that cannot tell them apart and the one that proves the
        // pass-through.
        let clamped: [f64; 3] = {
            let dst = apply_matrix(&WORKING_TO_SRGB, working);
            std::array::from_fn(|c| dst[c].clamp(0.0, 1.0))
        };
        let clamped_luma: f64 = (0..3).map(|c| SRGB_LUMA[c] * clamped[c]).sum();
        let neutral = probe[0] == probe[1] && probe[1] == probe[2];
        assert_eq!(
            (clamped_luma - want).abs() <= LUMA_TOLERANCE,
            neutral,
            "probe {probe:?}: a clamp is off by {:.4}, which {} the tolerance — \
             only the in-gamut neutral should be indistinguishable",
            (clamped_luma - want).abs(),
            if neutral { "should be inside" } else { "should exceed" }
        );
    }
}

/// An in-gamut colour must come through the compression untouched, or every
/// export would be quietly desaturated by a step that is only supposed to
/// rescue colour the destination cannot hold.
#[test]
fn colour_the_destination_can_hold_passes_through_unchanged() {
    // Deliberately modest: these convert to linear sRGB values inside 0..1.
    let probes: [[u8; 3]; 3] = [[180, 150, 120], [120, 130, 140], [96, 96, 96]];
    let out = render_to(&probes, OutputSpace::Srgb);

    for (i, probe) in probes.iter().enumerate() {
        let working: [f64; 3] = std::array::from_fn(|c| srgb_to_linear(probe[c] as f64 / 255.0));
        let dst = apply_matrix(&WORKING_TO_SRGB, working);
        assert!(
            dst.iter().all(|&c| (0.0..=1.0).contains(&c)),
            "probe {probe:?} is not in gamut, so it cannot test the pass-through"
        );

        let want: [u8; 3] = std::array::from_fn(|c| (linear_to_srgb(dst[c]) * 255.0).round() as u8);
        let got = pixel(&out, i);
        for c in 0..3 {
            assert!(
                (got[c] as i32 - want[c] as i32).abs() <= 2,
                "probe {probe:?} channel {c}: expected {} ± 2, got {} — in-gamut \
                 colour is being compressed",
                want[c],
                got[c]
            );
        }
    }
}

/// Rendering to the working space converts no primaries at all, so the result
/// is the transfer curve and nothing else. That makes it the one destination
/// with a closed form simple enough to pin exactly, and the one the 16-bit
/// default relies on being lossless.
#[test]
fn the_working_space_destination_only_applies_its_curve() {
    const ALPHA: f64 = 1.099_296_826_809_442;
    const BETA: f64 = 0.018_053_968_510_807;

    let encode = |linear: f64| -> f64 {
        if linear < BETA {
            linear * 4.5
        } else {
            ALPHA * linear.powf(0.45) - (ALPHA - 1.0)
        }
    };

    let probes: [[u8; 3]; 5] = [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [128, 128, 128],
        [12, 40, 200],
    ];
    let out = render_to(&probes, OutputSpace::Rec2020);

    for (i, probe) in probes.iter().enumerate() {
        let got = pixel(&out, i);
        for c in 0..3 {
            let linear = srgb_to_linear(probe[c] as f64 / 255.0);
            let want = (encode(linear) * 255.0).round() as i32;
            assert!(
                (got[c] as i32 - want).abs() <= 2,
                "probe {probe:?} channel {c}: expected {want} ± 2, got {} — the \
                 working space is being converted rather than just encoded",
                got[c]
            );
        }
    }
}

/// The primaries and the curve are separate halves of the transform, and mixing
/// them up is the easy mistake. sRGB and Display P3 share a curve and differ
/// only in primaries, so a neutral — which no set of primaries can move, since
/// all four spaces share a white point — has to come out identical, while a
/// saturated colour must not.
#[test]
fn the_destination_primaries_move_colour_and_leave_neutrals() {
    let probes: [[u8; 3]; 4] = [[128, 128, 128], [40, 40, 40], [255, 0, 0], [0, 200, 60]];
    let srgb = render_to(&probes, OutputSpace::Srgb);
    let p3 = render_to(&probes, OutputSpace::DisplayP3);

    for (i, probe) in probes.iter().enumerate() {
        let (a, b) = (pixel(&srgb, i), pixel(&p3, i));
        let neutral = probe[0] == probe[1] && probe[1] == probe[2];

        if neutral {
            for c in 0..3 {
                assert!(
                    (a[c] as i32 - b[c] as i32).abs() <= 1,
                    "neutral {probe:?} differs between sRGB {a:?} and Display P3 {b:?} — \
                     the two spaces do not share a white point any more"
                );
            }
        } else {
            let difference: i32 = (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).sum();
            assert!(
                difference > 4,
                "saturated {probe:?} encodes the same in sRGB {a:?} and Display P3 {b:?} — \
                 the destination matrix is not reaching the shader"
            );
        }
    }
}

/// Adobe RGB's curve is a pure power function where sRGB's has a linear toe, so
/// the two part company in the shadows and nowhere else. That is exactly where a
/// curve id crossing the language boundary wrongly would show up, and exactly
/// where it would be easiest to miss.
#[test]
fn each_destination_encodes_with_its_own_curve() {
    const ADOBE_GAMMA: f64 = 563.0 / 256.0;

    // Neutral, so the primaries cannot contribute and only the curve is left.
    let probes: [[u8; 3]; 3] = [[8, 8, 8], [64, 64, 64], [200, 200, 200]];
    let out = render_to(&probes, OutputSpace::AdobeRgb);

    for (i, probe) in probes.iter().enumerate() {
        let linear = srgb_to_linear(probe[0] as f64 / 255.0);
        let want = (linear.powf(1.0 / ADOBE_GAMMA) * 255.0).round() as i32;
        let got = pixel(&out, i);

        for c in 0..3 {
            assert!(
                (got[c] as i32 - want).abs() <= 2,
                "neutral {} channel {c}: Adobe RGB should encode to {want} ± 2, got {} \
                 (whole pixel {got:?})",
                probe[0],
                got[c]
            );
        }
    }

    // The shadow probe is where the two curves disagree most; if the curve id
    // were being ignored this is the assertion that would catch it.
    let srgb = render_to(&probes, OutputSpace::Srgb);
    assert!(
        (pixel(&srgb, 0)[0] as i32 - pixel(&out, 0)[0] as i32).abs() >= 2,
        "sRGB and Adobe RGB encode a deep shadow identically — one of them is \
         using the other's curve"
    );
}
