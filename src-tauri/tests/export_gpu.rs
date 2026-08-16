// src-tauri/tests/export_gpu.rs
//
// Layer 2: the mechanics of the export renderer rather than the colour maths —
// chunking, row padding, bit depth, progress and input validation. These are
// the parts that break on unusual image dimensions rather than unusual sliders.

mod common;

use common::{noise, rgb_ramp, solid, u8_to_u16};

use open_darkroom_lib::export_rendering::{
    render_image, render_image_with_chunk_rows, SlidersPayload, CHUNK_ROWS,
};
use open_darkroom_lib::image_opening::Rgba16Image;

fn render8(img: &Rgba16Image, sliders: &SlidersPayload) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_image(img.as_raw(), w, h, 4, sliders, 8, |_| {})).expect("render")
}

fn render_chunked(img: &Rgba16Image, chunk_rows: u32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        w,
        h,
        4,
        &SlidersPayload::default(),
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
    // 8 bytes per texel, so the padding boundary falls at different widths.
    for width in [1u32, 3, 31, 32, 33, 63, 97] {
        let img = rgb_ramp(width, 3);
        let out = pollster::block_on(render_image(
            img.as_raw(),
            width,
            3,
            4,
            &SlidersPayload::default(),
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
    let sixteen = pollster::block_on(render_image(img.as_raw(), 32, 16, 4, &sliders, 16, |_| {}))
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

    let err = pollster::block_on(render_image(&[0u16; 16], 4, 4, 4, &sliders, 8, |_| {}))
        .expect_err("buffer too small for 4×4 RGBA");
    assert!(err.contains("expected 64"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[], 0, 4, 4, &sliders, 8, |_| {}))
        .expect_err("zero width");
    assert!(err.contains("0 × 4"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[], 4, 0, 4, &sliders, 8, |_| {}))
        .expect_err("zero height");
    assert!(err.contains("4 × 0"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image_with_chunk_rows(
        &[0u16; 16],
        2,
        2,
        4,
        &sliders,
        8,
        0,
        |_| {},
    ))
    .expect_err("zero chunk size");
    assert!(err.contains("chunk_rows"), "unhelpful error: {err}");

    // A three-channel buffer sized as if it were four, and an unsupported
    // channel count — the stride is what the slicing arithmetic trusts.
    let err = pollster::block_on(render_image(&[0u16; 64], 4, 4, 3, &sliders, 8, |_| {}))
        .expect_err("buffer sized for RGBA, declared RGB");
    assert!(err.contains("expected 48"), "unhelpful error: {err}");

    let err = pollster::block_on(render_image(&[0u16; 32], 4, 4, 2, &sliders, 8, |_| {}))
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
