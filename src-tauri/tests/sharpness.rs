// src-tauri/tests/sharpness.rs
//
// The sharpness block — clarity, texture and the four unsharp mask controls.
//
// The unsharp mask renders: calcParams derives its sigma and amount from the
// slider and the view, calcSharpTexture extracts the band, and composite applies
// it. Those are covered here from three angles — the derived parameters, the
// shape of the halo on a real edge, and the chunk overlap the export needs so a
// blur that reads across a chunk boundary does not leave a seam.
//
// Clarity and texture have no band of their own yet, so for them "inert" is
// still the contract, and the last test holds them to it.

mod common;

use common::{develop, develop16, mixed_ramp, noise, run_calc_params, run_calc_sharp};

use open_darkroom_lib::export_rendering::{
    clarity_sigma, render_image_with_chunk_rows, sharp_margin_rows, sliders_to_bytes,
    view_to_bytes, SlidersPayload, ViewPayload, MAX_CLARITY_SIGMA, MIN_CLARITY_SIGMA,
    SLIDERS_BYTES, SLIDER_COUNT, VIEW_BYTES,
};
use open_darkroom_lib::image_opening::Rgba16Image;

/// Lane index of each sharpness field in the packed uniform. These are the tail
/// of the struct, after the 24 fields the develop chain already consumes.
const CLARITY: usize = 24;
const TEXTURE: usize = 25;
const USM_AMOUNT: usize = 26;
const USM_RADIUS: usize = 27;
const USM_LUMA_THRESHOLD: usize = 28;
const USM_DETAIL_THRESHOLD: usize = 29;

/// Every sharpness control pushed as far as the UI allows.
fn railed() -> SlidersPayload {
    SlidersPayload {
        clarity: 100.0,
        texture: -100.0,
        usm_amount: 300.0,
        usm_radius: 10.0,
        usm_luma_threshold: 100.0,
        usm_detail_threshold: 100.0,
        ..Default::default()
    }
}

fn lane(bytes: &[u8; SLIDERS_BYTES], index: usize) -> f32 {
    f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
}

// ── Defaults ──────────────────────────────────────────────────────────────────

/// Opening an image must not sharpen it. Amount 0 switches the mask off, which
/// is what makes radius free to start at a working value rather than a neutral
/// one — there is no radius that means "no blur".
#[test]
fn sharpness_defaults_are_neutral() {
    let d = SlidersPayload::default();

    assert_eq!(d.clarity, 0.0);
    assert_eq!(d.texture, 0.0);
    assert_eq!(d.usm_amount, 0.0);
    assert_eq!(d.usm_luma_threshold, 0.0);
    assert_eq!(d.usm_detail_threshold, 0.0);
    assert!(
        d.usm_radius > 0.0,
        "a zero radius would be a degenerate kernel, not a neutral one"
    );
}

// ── Packing ───────────────────────────────────────────────────────────────────

#[test]
fn sharpness_lands_in_its_own_lanes() {
    // Distinct values, so a field written into the wrong lane cannot pass by
    // coincidentally matching its neighbour.
    let s = SlidersPayload {
        clarity: 11.0,
        texture: 22.0,
        usm_amount: 33.0,
        usm_radius: 44.0,
        usm_luma_threshold: 55.0,
        usm_detail_threshold: 66.0,
        ..Default::default()
    };
    let bytes = sliders_to_bytes(&s);

    assert_eq!(lane(&bytes, CLARITY), 11.0);
    assert_eq!(lane(&bytes, TEXTURE), 22.0);
    assert_eq!(lane(&bytes, USM_AMOUNT), 33.0);
    assert_eq!(lane(&bytes, USM_RADIUS), 44.0);
    assert_eq!(lane(&bytes, USM_LUMA_THRESHOLD), 55.0);
    assert_eq!(lane(&bytes, USM_DETAIL_THRESHOLD), 66.0);
}

/// The sharpness block was appended, so every earlier lane must still hold what
/// it held before — the failure mode of an insertion is a silent one-lane shift
/// of everything downstream.
#[test]
fn sharpness_does_not_shift_the_lanes_before_it() {
    let neutral = sliders_to_bytes(&SlidersPayload::default());
    let maxed = sliders_to_bytes(&railed());

    assert_eq!(
        neutral[..CLARITY * 4],
        maxed[..CLARITY * 4],
        "railing the sharpness sliders moved one of the develop-chain fields"
    );
}

#[test]
fn the_uniform_is_padded_out_to_its_bound_size() {
    let bytes = sliders_to_bytes(&railed());

    assert_eq!(SLIDERS_BYTES % 16, 0, "a uniform struct is bound 16-aligned");
    assert!(SLIDERS_BYTES >= SLIDER_COUNT * 4);
    assert!(
        bytes[SLIDER_COUNT * 4..].iter().all(|&b| b == 0),
        "the tail past the last field is padding and must stay zeroed"
    );
}

// ── IPC payload ───────────────────────────────────────────────────────────────

/// The frontend hands `export_image` its whole `Sliders` object, so the camelCase
/// names the UI binds to have to deserialise into this struct. A rename on either
/// side turns into a failed export, not a wrong pixel.
#[test]
fn the_export_payload_carries_the_sharpness_keys() {
    let json = serde_json::json!({
        "invert": false,
        "redBlackPoint": 0.0,
        "greenBlackPoint": 0.0,
        "blueBlackPoint": 0.0,
        "redWhitePoint": 255.0,
        "greenWhitePoint": 255.0,
        "blueWhitePoint": 255.0,
        "rgbOutputBlack": 0.0,
        "rgbOutputWhite": 255.0,
        "redGamma": 1.0,
        "greenGamma": 1.0,
        "blueGamma": 1.0,
        "wbTemp": 5500.0,
        "wbTint": 0.0,
        "exposure": 0.0,
        "contrast": 0.0,
        "brightness": 0.0,
        "highlights": 0.0,
        "shadows": 0.0,
        "whites": 0.0,
        "blacks": 0.0,
        "saturation": 0.0,
        "vibrance": 0.0,
        "hue": 0.0,
        "clarity": 100.0,
        "texture": -100.0,
        "usmAmount": 300.0,
        "usmRadius": 10.0,
        "usmLumaThreshold": 100.0,
        "usmDetailThreshold": 100.0,
    });

    let parsed: SlidersPayload =
        serde_json::from_value(json).expect("the frontend payload must deserialise");

    assert_eq!(parsed, railed());
}

// ── The develop chain is untouched ────────────────────────────────────────────

#[test]
fn sharpness_does_not_change_the_computed_params() {
    let neutral = run_calc_params(&SlidersPayload::default());
    let maxed = run_calc_params(&railed());

    assert_eq!(
        neutral.0, maxed.0,
        "sharpness has its own SharpParams buffer; the develop chain's Params must be untouched"
    );
}

// ── Unsharp mask: the derived parameters ──────────────────────────────────────

#[test]
fn the_amount_is_off_until_the_slider_is_moved() {
    let sharp = run_calc_sharp(&SlidersPayload::default(), 1.0, 4000, 3000);
    assert_eq!(sharp.usm_amount, 0.0, "the default must switch the mask off");
}

#[test]
fn an_amount_of_one_hundred_percent_adds_the_whole_band() {
    let sliders = SlidersPayload {
        usm_amount: 100.0,
        ..Default::default()
    };
    let sharp = run_calc_sharp(&sliders, 1.0, 4000, 3000);

    assert!(
        (sharp.usm_amount - 1.0).abs() < 1e-5,
        "amount 100% should scale the band by 1.0, got {}",
        sharp.usm_amount
    );
}

#[test]
fn the_radius_is_measured_in_full_resolution_pixels() {
    // The same radius on a half-scale preview must cover half as many texture
    // pixels, or the preview would show a coarser mask than the export.
    let sliders = SlidersPayload {
        usm_amount: 100.0,
        usm_radius: 4.0,
        ..Default::default()
    };

    assert!((run_calc_sharp(&sliders, 1.0, 4000, 3000).usm_sigma - 4.0).abs() < 1e-5);
    assert!((run_calc_sharp(&sliders, 0.5, 4000, 3000).usm_sigma - 2.0).abs() < 1e-5);
    assert!((run_calc_sharp(&sliders, 0.25, 4000, 3000).usm_sigma - 1.0).abs() < 1e-5);
}

#[test]
fn zooming_out_fades_the_mask_instead_of_aliasing() {
    // Below a texture pixel the discrete kernel stops being sampled well enough
    // to mean anything. The sigma is floored so the kernel stays valid and the
    // amount is faded out instead, so the effect leaves the view smoothly.
    let sliders = SlidersPayload {
        usm_amount: 100.0,
        usm_radius: 1.0,
        ..Default::default()
    };

    let full = run_calc_sharp(&sliders, 1.0, 4000, 3000);
    let half = run_calc_sharp(&sliders, 0.5, 4000, 3000);
    let tiny = run_calc_sharp(&sliders, 0.05, 4000, 3000);

    assert!((full.usm_amount - 1.0).abs() < 1e-5, "1:1 must not fade");
    assert!(
        half.usm_amount < full.usm_amount && half.usm_amount > 0.0,
        "half scale should be partway through the fade, got {}",
        half.usm_amount
    );
    assert_eq!(tiny.usm_amount, 0.0, "a sub-pixel radius must fade to nothing");

    assert!(
        tiny.usm_sigma >= 0.7,
        "the sigma must stay above the floor even when faded out, got {}",
        tiny.usm_sigma
    );
}

#[test]
fn the_thresholds_are_scaled_out_of_slider_units() {
    let sliders = SlidersPayload {
        usm_amount: 100.0,
        usm_luma_threshold: 100.0,
        usm_detail_threshold: 100.0,
        ..Default::default()
    };
    let sharp = run_calc_sharp(&sliders, 1.0, 4000, 3000);

    // The luma gate spans the whole encoded range; the detail gate is scaled to
    // the band magnitudes real detail actually produces.
    assert!((sharp.usm_luma_threshold - 1.0).abs() < 1e-5);
    assert!((sharp.usm_detail_threshold - 0.1).abs() < 1e-5);

    let off = run_calc_sharp(&SlidersPayload::default(), 1.0, 4000, 3000);
    assert_eq!(off.usm_luma_threshold, 0.0);
    assert_eq!(off.usm_detail_threshold, 0.0);
}

// ── Unsharp mask: the rendered image ──────────────────────────────────────────

/// A vertical edge — the left half dark, the right half bright.
fn vertical_edge(width: u32, height: u32) -> Rgba16Image {
    let mut img = Rgba16Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = if x < width / 2 { 6400u16 } else { 45000u16 };
            img.put_pixel(x, y, image::Rgba([v, v, v, 65535]));
        }
    }
    img
}

fn sharpened(amount: f32, radius: f32) -> SlidersPayload {
    SlidersPayload {
        usm_amount: amount,
        usm_radius: radius,
        ..Default::default()
    }
}

#[test]
fn the_mask_overshoots_on_both_sides_of_an_edge() {
    // The signature of an unsharp mask: the dark side of an edge is driven below
    // the flat dark level and the bright side above the flat bright level. A
    // blur that was merely subtracted without a halo would do neither.
    let chart = vertical_edge(64, 4);
    let width = 64usize;

    let plain = develop(&chart, &SlidersPayload::default());
    let sharp = develop(&chart, &sharpened(150.0, 2.0));

    let red = |img: &[u8], x: usize| img[x * 3] as i32;

    let mid = width / 2;
    assert!(
        red(&sharp, mid - 1) < red(&plain, mid - 1),
        "the dark side of the edge must be driven darker"
    );
    assert!(
        red(&sharp, mid) > red(&plain, mid),
        "the bright side of the edge must be driven brighter"
    );

    // Far from the edge there is nothing to sharpen, so the flats must hold.
    assert_eq!(red(&sharp, 2), red(&plain, 2), "the dark flat must not move");
    assert_eq!(
        red(&sharp, width - 3),
        red(&plain, width - 3),
        "the bright flat must not move"
    );
}

#[test]
fn a_negative_amount_softens_the_same_edge() {
    let chart = vertical_edge(64, 4);
    let mid = 32usize;

    let plain = develop(&chart, &SlidersPayload::default());
    let softened = develop(&chart, &sharpened(-100.0, 2.0));

    let red = |img: &[u8], x: usize| img[x * 3] as i32;

    assert!(
        red(&softened, mid - 1) > red(&plain, mid - 1),
        "a negative amount must lift the dark side rather than deepen it"
    );
    assert!(
        red(&softened, mid) < red(&plain, mid),
        "and pull the bright side down"
    );
}

#[test]
fn a_wider_radius_reaches_further_from_the_edge() {
    let chart = vertical_edge(96, 4);
    let mid = 48usize;

    let plain = develop(&chart, &SlidersPayload::default());
    let narrow = develop(&chart, &sharpened(150.0, 1.0));
    let wide = develop(&chart, &sharpened(150.0, 6.0));

    let moved = |img: &[u8], x: usize| (img[x * 3] as i32 - plain[x * 3] as i32).abs();

    // Eight pixels out, a one-pixel radius has nothing left to say.
    assert_eq!(moved(&narrow, mid - 9), 0, "radius 1 must not reach this far");
    assert!(
        moved(&wide, mid - 9) > 0,
        "radius 6 must still be moving pixels here"
    );
}

#[test]
fn sharpening_does_not_shift_hue() {
    // The band is applied by scaling the colour rather than adding to each
    // channel, so the ratios between channels — and therefore the hue and
    // saturation — survive. Adding instead would wash out the bright side of the
    // edge and over-saturate the dark side.
    //
    // Read at 16 bits: the property is exact in the shader, and at 8 bits the
    // quantisation of a dark channel is coarse enough to swamp it.
    let mut chart = Rgba16Image::new(64, 4);
    for y in 0..4 {
        for x in 0..64 {
            let px = if x < 32 {
                image::Rgba([20000u16, 5000, 3200, 65535])
            } else {
                image::Rgba([50000u16, 12500, 8000, 65535])
            };
            chart.put_pixel(x, y, px);
        }
    }

    let plain = develop16(&chart, &SlidersPayload::default());
    let sharp = develop16(&chart, &sharpened(80.0, 2.0));

    let mut checked = 0;
    for x in 28usize..36 {
        let at = |img: &[u16], c: usize| img[x * 3 + c] as f32;

        // A clipped channel has lost the direction the ratio lives in; the
        // overshoot test above is what covers those.
        if at(&sharp, 0) >= 65535.0 || at(&sharp, 0) <= 0.0 {
            continue;
        }

        for c in [1usize, 2] {
            let plain_ratio = at(&plain, c) / at(&plain, 0);
            let sharp_ratio = at(&sharp, c) / at(&sharp, 0);
            assert!(
                (sharp_ratio - plain_ratio).abs() < 0.005,
                "channel {c}/red drifted at x={x}: {sharp_ratio} vs {plain_ratio}"
            );
        }
        checked += 1;
    }

    assert!(checked >= 6, "only {checked} pixels were testable");
}

#[test]
fn amount_zero_renders_exactly_as_if_the_mask_were_absent() {
    // Radius and the thresholds are free to sit anywhere when the amount is off;
    // nothing about the render may depend on them.
    let chart = mixed_ramp(64);

    let plain = develop(&chart, &SlidersPayload::default());
    let idle = develop(
        &chart,
        &SlidersPayload {
            usm_amount: 0.0,
            usm_radius: 9.0,
            usm_luma_threshold: 60.0,
            usm_detail_threshold: 40.0,
            ..Default::default()
        },
    );

    assert_eq!(plain, idle, "amount 0 must be a bit-exact no-op");
}

// ── Chunked export ────────────────────────────────────────────────────────────

#[test]
fn chunk_size_does_not_change_a_sharpened_render() {
    // The blur reads rows either side of the pixel it is writing, so every chunk
    // has to be rendered with a margin of neighbours that is then discarded.
    // Without it each boundary blurs against a clamped edge and leaves a seam.
    let img = noise(48, 200, 11);
    let (w, h) = img.dimensions();
    let sliders = sharpened(200.0, 8.0);

    let whole = pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        w,
        h,
        &sliders,
        8,
        h,
        |_| {},
    ))
    .expect("render");

    for chunk_rows in [7u32, 16, 31, 64, 199] {
        let chunked = pollster::block_on(render_image_with_chunk_rows(
            img.as_raw(),
            w,
            h,
            &sliders,
            8,
            chunk_rows,
            |_| {},
        ))
        .expect("render");

        assert_eq!(
            whole, chunked,
            "sharpened render differs when chunked at {chunk_rows} rows"
        );
    }
}

#[test]
fn the_overlap_is_one_radius_and_disappears_when_the_mask_is_off() {
    // The vertical pass reads the horizontal pass at ±radius rows, and the
    // horizontal pass reads only its own row — so one radius is enough, not two.
    assert_eq!(sharp_margin_rows(&SlidersPayload::default(), 4000, 3000), 0);
    assert_eq!(sharp_margin_rows(&sharpened(100.0, 1.0), 4000, 3000), 3);
    assert_eq!(sharp_margin_rows(&sharpened(100.0, 4.0), 4000, 3000), 12);

    // Clamped to the widest kernel the shader will build.
    assert_eq!(sharp_margin_rows(&sharpened(100.0, 10.0), 4000, 3000), 30);

    // Below the sigma floor the kernel still has a real width.
    assert_eq!(sharp_margin_rows(&sharpened(100.0, 0.5), 4000, 3000), 3);

    // Texture's upper sigma is fixed, clarity's follows the frame, and the widest
    // band in play is the one that sets the margin.
    assert_eq!(sharp_margin_rows(&with_texture(100.0), 4000, 3000), 15);
    assert_eq!(sharp_margin_rows(&with_clarity(100.0), 4000, 3000), 90);
    assert_eq!(sharp_margin_rows(&with_clarity(100.0), 800, 600), 60);

    let everything = SlidersPayload {
        clarity: 100.0,
        texture: 100.0,
        usm_amount: 100.0,
        usm_radius: 2.0,
        ..Default::default()
    };
    assert_eq!(sharp_margin_rows(&everything, 4000, 3000), 90);
}

#[test]
fn chunk_size_does_not_change_a_clarity_render() {
    // Clarity's kernel reaches an order of magnitude further than the unsharp
    // mask's, so it is the band that actually stresses the overlap.
    let img = noise(48, 400, 13);
    let (w, h) = img.dimensions();
    let sliders = SlidersPayload {
        clarity: 100.0,
        texture: 80.0,
        usm_amount: 120.0,
        usm_radius: 3.0,
        ..Default::default()
    };

    let whole = pollster::block_on(render_image_with_chunk_rows(
        img.as_raw(),
        w,
        h,
        &sliders,
        8,
        h,
        |_| {},
    ))
    .expect("render");

    for chunk_rows in [13u32, 64, 128, 399] {
        let chunked = pollster::block_on(render_image_with_chunk_rows(
            img.as_raw(),
            w,
            h,
            &sliders,
            8,
            chunk_rows,
            |_| {},
        ))
        .expect("render");

        assert_eq!(
            whole, chunked,
            "clarity render differs when chunked at {chunk_rows} rows"
        );
    }
}

// ── Texture ───────────────────────────────────────────────────────────────────

/// Vertical stripes of the given period, alternating around mid-grey.
fn stripes(width: u32, height: u32, period: u32) -> Rgba16Image {
    let mut img = Rgba16Image::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = if (x / (period / 2)) % 2 == 0 { 26000u16 } else { 39000u16 };
            img.put_pixel(x, y, image::Rgba([v, v, v, 65535]));
        }
    }
    img
}

/// How far a render moved from the unsharpened one, summed over the red channel.
fn total_movement(plain: &[u8], other: &[u8]) -> i64 {
    plain
        .iter()
        .zip(other)
        .step_by(3)
        .map(|(a, b)| (*a as i64 - *b as i64).abs())
        .sum()
}

fn with_texture(amount: f32) -> SlidersPayload {
    SlidersPayload {
        texture: amount,
        ..Default::default()
    }
}

#[test]
fn texture_acts_on_mid_scale_detail() {
    let chart = stripes(96, 4, 6);
    let plain = develop(&chart, &SlidersPayload::default());
    let lifted = develop(&chart, &with_texture(100.0));

    assert!(
        total_movement(&plain, &lifted) > 0,
        "texture must act on detail at its own scale"
    );
}

#[test]
fn texture_passes_over_the_finest_detail() {
    // The band-pass has a lower cutoff as well as an upper one, and that is what
    // separates it from a sharpening radius: a one-pixel pattern is grain, and
    // texture must leave it where an unsharp mask would amplify it.
    let grain = stripes(96, 4, 2);
    let plain = develop(&grain, &SlidersPayload::default());

    let textured = develop(&grain, &with_texture(100.0));
    let sharpened_grain = develop(&grain, &sharpened(100.0, 1.0));

    let texture_moved = total_movement(&plain, &textured);
    let usm_moved = total_movement(&plain, &sharpened_grain);

    assert!(
        texture_moved * 4 < usm_moved,
        "texture moved {texture_moved} on one-pixel detail against the mask's {usm_moved}; \
         the lower cutoff is not doing its job"
    );
}

#[test]
fn texture_reverses_with_the_slider() {
    let chart = stripes(96, 4, 6);
    let plain = develop(&chart, &SlidersPayload::default());
    let up = develop(&chart, &with_texture(100.0));
    let down = develop(&chart, &with_texture(-100.0));

    // Sample a column inside a bright stripe: lifting detail must brighten it and
    // flattening detail must pull it back toward its neighbours.
    let x = 3usize;
    assert!(up[x * 3] > plain[x * 3], "positive texture must lift the peak");
    assert!(down[x * 3] < plain[x * 3], "negative texture must flatten it");
}

// ── Clarity ───────────────────────────────────────────────────────────────────

/// A vertical step between two levels, wide enough for a large-radius kernel.
fn step_edge(width: u32, dark: u16, bright: u16) -> Rgba16Image {
    let mut img = Rgba16Image::new(width, 4);
    for y in 0..4 {
        for x in 0..width {
            let v = if x < width / 2 { dark } else { bright };
            img.put_pixel(x, y, image::Rgba([v, v, v, 65535]));
        }
    }
    img
}

fn with_clarity(amount: f32) -> SlidersPayload {
    SlidersPayload {
        clarity: amount,
        ..Default::default()
    }
}

#[test]
fn clarity_steepens_a_broad_edge() {
    // Its radius is tens of pixels, so unlike the unsharp mask it works well away
    // from the edge itself — that reach is what makes it local contrast rather
    // than sharpening.
    let chart = step_edge(200, 22000, 42000);
    let mid = 100usize;

    let plain = develop(&chart, &SlidersPayload::default());
    let punchy = develop(&chart, &with_clarity(100.0));

    assert!(
        punchy[(mid - 10) * 3] < plain[(mid - 10) * 3],
        "the dark side must deepen ten pixels out"
    );
    assert!(
        punchy[(mid + 10) * 3] > plain[(mid + 10) * 3],
        "and the bright side must lift"
    );
}

#[test]
fn clarity_radius_scales_with_the_image_between_its_clamps() {
    // A fraction of the short edge, so the slider means the same thing on a 12
    // and a 60 megapixel file — but clamped, or it would collide with texture's
    // scale on small images and produce unusable halos on huge ones.
    assert_eq!(clarity_sigma(6000, 4000), 40.0);
    assert_eq!(clarity_sigma(4000, 3000), 30.0);
    assert_eq!(clarity_sigma(800, 600), MIN_CLARITY_SIGMA);
    assert_eq!(clarity_sigma(20000, 15000), MAX_CLARITY_SIGMA);
}

#[test]
fn clarity_backs_off_at_the_endpoints() {
    // The extremes belong to the output black and white points; driving a broad
    // local-contrast move into them only crushes them.
    let midtones = step_edge(200, 22000, 42000);
    let shadows = step_edge(200, 1200, 4200);

    let mid_plain = develop(&midtones, &SlidersPayload::default());
    let mid_punchy = develop(&midtones, &with_clarity(100.0));
    let dark_plain = develop(&shadows, &SlidersPayload::default());
    let dark_punchy = develop(&shadows, &with_clarity(100.0));

    let mid_moved = total_movement(&mid_plain, &mid_punchy);
    let dark_moved = total_movement(&dark_plain, &dark_punchy);

    assert!(
        dark_moved * 2 < mid_moved,
        "clarity moved {dark_moved} deep in the shadows against {mid_moved} in the midtones; \
         the mask is not protecting the endpoints"
    );
}

#[test]
fn clarity_saturates_rather_than_growing_without_bound() {
    // A halo tens of pixels wide is the over-processed look the effect is
    // infamous for, so the excursion approaches a ceiling instead of scaling with
    // the slider all the way to the rail.
    let chart = step_edge(200, 22000, 42000);

    let plain = develop(&chart, &SlidersPayload::default());
    let half = develop(&chart, &with_clarity(50.0));
    let full = develop(&chart, &with_clarity(100.0));

    let half_moved = total_movement(&plain, &half);
    let full_moved = total_movement(&plain, &full);

    assert!(full_moved > half_moved, "the slider must still do something");
    assert!(
        full_moved < half_moved * 2,
        "doubling the slider moved {half_moved} to {full_moved}, which is not saturating"
    );
}

/// A smooth sinusoidal ramp of the given period, around mid-grey.
fn soft_wave(width: u32, period: f32) -> Rgba16Image {
    let mut img = Rgba16Image::new(width, 4);
    for y in 0..4 {
        for x in 0..width {
            let phase = (x as f32 / period) * std::f32::consts::TAU;
            let v = (32000.0 + 9000.0 * phase.sin()) as u16;
            img.put_pixel(x, y, image::Rgba([v, v, v, 65535]));
        }
    }
    img
}

#[test]
fn clarity_and_texture_act_on_scales_the_other_cannot() {
    // The two bands share a cutoff, so between them they partition the range of
    // scales rather than overlapping. Both directions have to hold, or one
    // control is quietly doing the other's job — and a clarity that reached down
    // into texture's range would lift film grain along with local contrast.
    let broad = soft_wave(240, 100.0);
    let fine = stripes(200, 4, 6);

    let check = |chart: &Rgba16Image| {
        let plain = develop(chart, &SlidersPayload::default());
        (
            total_movement(&plain, &develop(chart, &with_texture(100.0))),
            total_movement(&plain, &develop(chart, &with_clarity(100.0))),
        )
    };

    let (texture_on_broad, clarity_on_broad) = check(&broad);
    let (texture_on_fine, clarity_on_fine) = check(&fine);

    assert!(
        texture_on_broad * 4 < clarity_on_broad,
        "texture moved {texture_on_broad} on hundred-pixel structure against clarity's \
         {clarity_on_broad}; it is reaching above its upper cutoff"
    );
    assert!(
        clarity_on_fine * 4 < texture_on_fine,
        "clarity moved {clarity_on_fine} on six-pixel stripes against texture's \
         {texture_on_fine}; it is reaching below its lower cutoff"
    );
}

#[test]
fn clarity_and_texture_are_separate_channels() {
    // Both bands live in the same detail texture, so a wiring mistake that
    // pointed one at the other's channel would still produce a plausible image.
    let chart = soft_wave(240, 40.0);

    let textured = develop(&chart, &with_texture(100.0));
    let clarified = develop(&chart, &with_clarity(100.0));

    assert_ne!(
        textured, clarified,
        "texture and clarity must not be reading the same band"
    );
}

// ── View ──────────────────────────────────────────────────────────────────────
//
// The radius is specified in full-resolution image pixels, so a render working
// on a downscaled surface has to scale it. The ratio rides in its own uniform
// rather than in `Sliders`, because it tracks the view and not an edit — it
// changes on zoom, while the sliders change only when the user moves one.

#[test]
fn the_view_defaults_to_full_resolution() {
    // The export renders the whole image at native size, so it never scales.
    assert_eq!(ViewPayload::default().render_scale, 1.0);
}

#[test]
fn the_view_packs_the_render_scale_into_the_first_lane() {
    let bytes = view_to_bytes(&ViewPayload {
        render_scale: 0.25,
        full_width: 6000.0,
        full_height: 4000.0,
    });

    assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0.25);
    assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 6000.0);
    assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 4000.0);
}

#[test]
fn the_view_zeroes_its_tail_padding() {
    // Three f32 rounded up to the 16 bytes a uniform binds at. The padding is
    // never read, but leaving it undefined would make byte-for-byte comparisons
    // of the uniform meaningless.
    let bytes = view_to_bytes(&ViewPayload {
        render_scale: 0.5,
        full_width: 100.0,
        full_height: 80.0,
    });

    assert_eq!(bytes.len(), VIEW_BYTES);
    assert!(
        bytes[12..].iter().all(|&b| b == 0),
        "tail padding must be zeroed, got {:?}",
        &bytes[12..]
    );
}
