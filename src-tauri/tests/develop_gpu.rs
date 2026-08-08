// src-tauri/tests/develop_gpu.rs
//
// Layer 2: the develop chain running on a real GPU, through the same WGSL the
// app ships. Covers calcParams.wgsl (matrix construction), develop.wgsl (the
// per-pixel chain) and histogram.wgsl (the preview's histogram).
//
// GPU maths is not bit-identical across vendors, so numeric comparisons use
// tolerances. Structural invariants — vote totals, orderings — are exact.

mod common;

use common::{
    develop, develop16, develop_patch, gray_ramp, primaries, rgb_ramp, run_calc_params,
    run_histogram, solid, through_linear, u8_to_u16, weighted_mean_bin, HIST_VOTE,
};

use open_darkroom_lib::export_rendering::SlidersPayload;
use open_darkroom_lib::image_opening::{Rgba16Image, HIST_BINS};

// ── calcParams: matrix construction ───────────────────────────────────────────

#[test]
fn default_sliders_build_identity_matrices() {
    let p = run_calc_params(&SlidersPayload::default());

    let identity = [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];

    for (i, (&got, &want)) in p.dr_matrix().iter().zip(identity.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-5,
            "dr_matrix[{i}]: expected {want}, got {got}"
        );
    }

    for (i, (&got, &want)) in p.hue_sat_matrix().iter().zip(identity.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-5,
            "hueSat_matrix[{i}]: expected {want}, got {got}"
        );
    }

    // At the neutral reference temperature the adaptation cancels exactly, so
    // this must be identity to f32 rounding — not merely close to it. A looser
    // bound here would hide a tint on every unedited image.
    let wb = p.wb_matrix();
    for col in 0..3 {
        for row in 0..3 {
            let want = if col == row { 1.0 } else { 0.0 };
            assert!(
                (wb[col][row] - want).abs() < 1e-5,
                "wb_matrix[{col}][{row}]: expected {want}, got {}",
                wb[col][row]
            );
        }
    }

    assert_eq!(p.gammas(), [1.0, 1.0, 1.0]);
    assert_eq!(p.exposure(), 0.0);
    assert_eq!(p.contrast(), 0.0);
    assert_eq!(p.vibrance(), 0.0);
}

/// Every slider but exposure arrives as -100..100 from the UI and is scaled to
/// the range the shader works in. Exposure is the exception: it is measured in
/// stops, so it passes straight through.
#[test]
fn percentage_sliders_are_scaled_to_the_shader_range() {
    let p = run_calc_params(&SlidersPayload {
        contrast: 50.0,
        vibrance: 25.0,
        exposure: 1.5,
        brightness: 40.0,
        highlights: 100.0,
        shadows: -50.0,
        whites: 100.0,
        blacks: -100.0,
        ..Default::default()
    });

    assert!((p.contrast() - 0.5).abs() < 1e-6, "contrast is a percentage");
    assert!((p.vibrance() - 0.25).abs() < 1e-6, "vibrance is a percentage");
    assert!((p.exposure() - 1.5).abs() < 1e-6, "exposure passes through");

    // The tone-region amplitudes are budgeted so that no combination of the four
    // can fold the tone curve back on itself — see calcParams.wgsl.
    assert!((p.brightness() - 0.10).abs() < 1e-6, "brightness × 0.25");
    assert!((p.highlights() - 0.20).abs() < 1e-6, "highlights × 0.20");
    assert!((p.shadows() + 0.10).abs() < 1e-6, "shadows × 0.20");
    assert!((p.whites() - 0.07).abs() < 1e-6, "whites × 0.07");
    assert!((p.blacks() + 0.07).abs() < 1e-6, "blacks × 0.07");
}

#[test]
fn gamma_is_clamped_away_from_zero() {
    // 1/gamma is evaluated per pixel, so a zero here would poison the image.
    let p = run_calc_params(&SlidersPayload {
        red_gamma: 0.0,
        green_gamma: -5.0,
        blue_gamma: 0.001,
        ..Default::default()
    });

    for (channel, gamma) in ["red", "green", "blue"].iter().zip(p.gammas()) {
        assert!(
            gamma >= 0.01,
            "{channel} gamma was not clamped: {gamma}"
        );
        assert!(gamma.is_finite(), "{channel} gamma is not finite");
    }
}

#[test]
fn inversion_flips_the_darkroom_matrix_slope() {
    let normal = run_calc_params(&SlidersPayload::default());
    let inverted = run_calc_params(&SlidersPayload {
        invert: true,
        ..Default::default()
    });

    // Column-major: lane 0 is the red scale, lane 12 the red offset.
    assert!(normal.dr_matrix()[0] > 0.0, "normal slope is positive");
    assert!(inverted.dr_matrix()[0] < 0.0, "inverted slope is negative");
    assert!(
        (inverted.dr_matrix()[12] - 1.0).abs() < 1e-5,
        "inverted red channel should start at the output white point"
    );
}

/// The parameters must stay finite for every slider position the UI can reach.
/// A non-finite matrix entry propagates to every pixel of the image.
#[test]
fn extreme_sliders_never_produce_non_finite_params() {
    let cases: Vec<(&str, SlidersPayload)> = vec![
        (
            "black and white points coincident",
            SlidersPayload {
                red_black_point: 128.0,
                red_white_point: 128.0,
                ..Default::default()
            },
        ),
        (
            "all points coincident at zero",
            SlidersPayload {
                red_black_point: 0.0,
                green_black_point: 0.0,
                blue_black_point: 0.0,
                red_white_point: 0.0,
                green_white_point: 0.0,
                blue_white_point: 0.0,
                ..Default::default()
            },
        ),
        (
            "inverted point order",
            SlidersPayload {
                red_black_point: 255.0,
                red_white_point: 0.0,
                ..Default::default()
            },
        ),
        (
            "output range collapsed",
            SlidersPayload {
                rgb_output_black: 200.0,
                rgb_output_white: 200.0,
                ..Default::default()
            },
        ),
        (
            "coldest white balance",
            SlidersPayload {
                wb_temp: 1000.0,
                wb_tint: -150.0,
                ..Default::default()
            },
        ),
        (
            "hottest white balance",
            SlidersPayload {
                wb_temp: 25000.0,
                wb_tint: 150.0,
                ..Default::default()
            },
        ),
        (
            "white balance below the clamp",
            SlidersPayload {
                wb_temp: 0.0,
                ..Default::default()
            },
        ),
        (
            "saturation removed, hue rotated",
            SlidersPayload {
                saturation: -100.0,
                hue: 180.0,
                ..Default::default()
            },
        ),
        (
            "everything at the rail",
            SlidersPayload {
                invert: true,
                red_gamma: 0.0,
                green_gamma: 0.0,
                blue_gamma: 0.0,
                exposure: 10.0,
                contrast: 100.0,
                brightness: 100.0,
                highlights: 100.0,
                shadows: 100.0,
                whites: 100.0,
                blacks: 100.0,
                saturation: 100.0,
                vibrance: 100.0,
                hue: 360.0,
                ..Default::default()
            },
        ),
    ];

    let mut failures = Vec::new();
    for (label, sliders) in cases {
        let p = run_calc_params(&sliders);
        let bad: Vec<String> = p
            .significant_values()
            .into_iter()
            .filter(|(_, v)| !v.is_finite())
            .map(|(lane, v)| format!("lane {lane} = {v}"))
            .collect();
        if !bad.is_empty() {
            failures.push(format!("  {label}: {}", bad.join(", ")));
        }
    }

    assert!(
        failures.is_empty(),
        "calcParams produced non-finite values:\n{}",
        failures.join("\n")
    );
}

// ── develop.wgsl: the per-pixel chain ─────────────────────────────────────────

#[test]
fn default_sliders_are_an_identity_transform() {
    let img = rgb_ramp(64, 8);
    let out = develop(&img, &SlidersPayload::default());

    assert_eq!(out.len(), 64 * 8 * 3);

    for i in 0..(64 * 8) {
        for c in 0..3 {
            let expected = (img.as_raw()[i * 4 + c] / 257) as i32;
            let actual = out[i * 3 + c] as i32;
            assert!(
                (expected - actual).abs() <= 2,
                "pixel {i} channel {c}: expected ~{expected}, got {actual}"
            );
        }
    }
}

#[test]
fn primaries_stay_on_their_own_channels() {
    // Catches a transposed matrix, which an all-grey test would miss entirely.
    let out = develop(&primaries(), &SlidersPayload::default());

    let patch = |i: usize| [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]];

    assert_eq!(patch(0), [0, 0, 0], "black");
    assert!(patch(1)[0] > 250 && patch(1)[1] < 5 && patch(1)[2] < 5, "red");
    assert!(patch(2)[1] > 250 && patch(2)[0] < 5 && patch(2)[2] < 5, "green");
    assert!(patch(3)[2] > 250 && patch(3)[0] < 5 && patch(3)[1] < 5, "blue");
    assert!(patch(7).iter().all(|&c| c > 250), "white");
}

#[test]
fn exposure_scales_linear_light() {
    let sliders = SlidersPayload {
        exposure: 1.0,
        ..Default::default()
    };

    for value in [32u8, 64, 128, 180] {
        let got = develop_patch([value; 3], &sliders);
        let expected = through_linear(value, |l| l * 2.0);

        for (c, &actual) in got.iter().enumerate() {
            assert!(
                (actual as i32 - expected as i32).abs() <= 2,
                "value {value} channel {c}: +1EV should give ~{expected}, got {actual}"
            );
        }
    }
}

#[test]
fn per_channel_gamma_lifts_midtones() {
    let sliders = SlidersPayload {
        red_gamma: 2.2,
        ..Default::default()
    };

    let got = develop_patch([128, 128, 128], &sliders);
    let expected = through_linear(128, |l| l.powf(1.0 / 2.2));

    assert!(
        (got[0] as i32 - expected as i32).abs() <= 2,
        "red with gamma 2.2: expected ~{expected}, got {}",
        got[0]
    );
    assert!(
        got[0] > got[1],
        "only red had gamma applied: {got:?}"
    );
    assert!(
        (got[1] as i32 - 128).abs() <= 2,
        "green should be untouched, got {}",
        got[1]
    );
}

/// Inverting twice must return the original image — the core guarantee of the
/// negative workflow.
///
/// The round trip runs at 16 bits deliberately. Inversion happens in linear
/// light, so shadow detail lands in the top few sRGB code values: an 8-bit
/// intermediate collapses input 4 onto 255, and inverting 255 gives 0 rather
/// than 4. That is a real property of the pipeline, and the reason inverting a
/// negative is worth doing at 16-bit depth.
#[test]
fn inversion_is_its_own_inverse() {
    let sliders = SlidersPayload {
        invert: true,
        ..Default::default()
    };

    let original = rgb_ramp(64, 4);
    let once = develop16(&original, &sliders);

    // Feed the inverted result back in as a 16-bit source.
    let round_two = Rgba16Image::from_fn(64, 4, |x, y| {
        let i = (y * 64 + x) as usize * 3;
        image::Rgba([once[i], once[i + 1], once[i + 2], u16::MAX])
    });
    let twice = develop16(&round_two, &sliders);

    // Compare in linear light, which is where the inversion actually happens;
    // in sRGB code values the error near black looks enormous because the
    // transfer curve has a slope of ~13 there.
    //
    // The budget is one f16 ULP at the white end: f16 spacing just below 1.0 is
    // 2^-11 ≈ 4.9e-4, and converting that sRGB step to linear near white
    // multiplies it by d(linear)/d(sRGB) = 2.4/1.055 ≈ 2.3, giving ~1.1e-3.
    // That is the texture format's floor, not an error in the chain.
    const TOLERANCE: f64 = 1.5e-3;

    for i in 0..(64 * 4) {
        for c in 0..3 {
            let expected = common::srgb_to_linear(original.as_raw()[i * 4 + c] as f64 / 65535.0);
            let actual = common::srgb_to_linear(twice[i * 3 + c] as f64 / 65535.0);
            assert!(
                (expected - actual).abs() < TOLERANCE,
                "pixel {i} channel {c}: double inversion gave linear {actual:.6}, \
                 expected {expected:.6}"
            );
        }
    }
}

#[test]
fn inversion_reverses_the_tone_order() {
    let sliders = SlidersPayload {
        invert: true,
        ..Default::default()
    };
    let out = develop(&gray_ramp(64, 1), &sliders);

    assert!(out[0] > 250, "black input becomes white, got {}", out[0]);
    assert!(out[63 * 3] < 5, "white input becomes black, got {}", out[63 * 3]);
}

#[test]
fn a_ramp_stays_ordered_under_every_tone_control() {
    let cases: Vec<(&str, SlidersPayload)> = vec![
        ("default", SlidersPayload::default()),
        (
            "exposure +1",
            SlidersPayload { exposure: 1.0, ..Default::default() },
        ),
        (
            "exposure -1",
            SlidersPayload { exposure: -1.0, ..Default::default() },
        ),
        (
            "contrast 50",
            SlidersPayload { contrast: 50.0, ..Default::default() },
        ),
        (
            "contrast -100",
            SlidersPayload { contrast: -100.0, ..Default::default() },
        ),
        (
            "brightness +40",
            SlidersPayload { brightness: 40.0, ..Default::default() },
        ),
        (
            "gamma 2.2",
            SlidersPayload {
                red_gamma: 2.2,
                green_gamma: 2.2,
                blue_gamma: 2.2,
                ..Default::default()
            },
        ),
        (
            "narrowed points",
            SlidersPayload {
                red_black_point: 20.0,
                green_black_point: 20.0,
                blue_black_point: 20.0,
                red_white_point: 200.0,
                green_white_point: 200.0,
                blue_white_point: 200.0,
                ..Default::default()
            },
        ),
        (
            "shadows and highlights lifted",
            SlidersPayload {
                shadows: 100.0,
                highlights: 100.0,
                ..Default::default()
            },
        ),
        (
            "every tone control at a rail",
            SlidersPayload {
                contrast: -100.0,
                brightness: 50.0,
                highlights: -100.0,
                shadows: 100.0,
                whites: -100.0,
                blacks: 100.0,
                ..Default::default()
            },
        ),
    ];

    for (label, sliders) in cases {
        let out = develop(&gray_ramp(256, 1), &sliders);
        for x in 1..256 {
            for c in 0..3 {
                let prev = out[(x - 1) * 3 + c] as i32;
                let curr = out[x * 3 + c] as i32;
                assert!(
                    curr >= prev - 1,
                    "{label}: channel {c} dipped at column {x} ({prev} → {curr})"
                );
            }
        }
    }
}

#[test]
fn black_and_white_points_remap_the_endpoints() {
    // Pulling the white point down to 128 should drive everything at or above
    // 128 to full white, and leave black where it is.
    let sliders = SlidersPayload {
        red_white_point: 128.0,
        green_white_point: 128.0,
        blue_white_point: 128.0,
        ..Default::default()
    };

    assert_eq!(develop_patch([0, 0, 0], &sliders), [0, 0, 0], "black holds");

    let mid = develop_patch([128, 128, 128], &sliders);
    assert!(
        mid.iter().all(|&c| c > 250),
        "the new white point should clip to white, got {mid:?}"
    );
}

// ── histogram.wgsl ────────────────────────────────────────────────────────────

#[test]
fn every_pixel_casts_exactly_one_vote() {
    // The vote is split between two adjacent bins, so the totals are exact
    // integers no matter how the fractional weight falls.
    let img = rgb_ramp(64, 64);
    let bins = run_histogram(&img, &SlidersPayload::default());
    let expected = 64u64 * 64 * HIST_VOTE as u64;

    for (channel, arr) in ["r", "g", "b"].iter().zip(bins.iter()) {
        let total: u64 = arr.iter().map(|&c| c as u64).sum();
        assert_eq!(total, expected, "{channel} channel lost or gained weight");
    }
}

#[test]
fn vote_totals_hold_under_extreme_sliders() {
    let img = rgb_ramp(32, 32);
    let expected = 32u64 * 32 * HIST_VOTE as u64;

    let cases = [
        SlidersPayload { exposure: 5.0, ..Default::default() },
        SlidersPayload { exposure: -5.0, ..Default::default() },
        SlidersPayload { invert: true, ..Default::default() },
        SlidersPayload { contrast: 100.0, saturation: -100.0, ..Default::default() },
    ];

    for (i, sliders) in cases.iter().enumerate() {
        let bins = run_histogram(&img, sliders);
        for (channel, arr) in ["r", "g", "b"].iter().zip(bins.iter()) {
            let total: u64 = arr.iter().map(|&c| c as u64).sum();
            assert_eq!(total, expected, "case {i}, {channel} channel");
        }
    }
}

#[test]
fn a_solid_patch_lands_on_its_own_bin() {
    for value in [0u8, 1, 64, 128, 200, 255] {
        let img = solid(16, 16, [u8_to_u16(value); 4]);
        let bins = run_histogram(&img, &SlidersPayload::default());

        let mean = weighted_mean_bin(&bins[0]);
        assert!(
            (mean - value as f64).abs() < 1.0,
            "solid {value} produced mean bin {mean:.2}"
        );

        // Weight is split across at most two adjacent bins.
        let occupied: Vec<usize> = (0..HIST_BINS).filter(|&i| bins[0][i] > 0).collect();
        assert!(
            occupied.len() <= 2,
            "solid {value} spread across {} bins: {occupied:?}",
            occupied.len()
        );
    }
}

/// The histogram shader carries its own copy of the develop chain. If the two
/// drift, the panel describes an image the canvas never showed.
#[test]
fn histogram_agrees_with_the_developed_image() {
    let img = rgb_ramp(64, 64);

    let cases: Vec<(&str, SlidersPayload)> = vec![
        ("default", SlidersPayload::default()),
        ("exposure +1", SlidersPayload { exposure: 1.0, ..Default::default() }),
        ("contrast 60", SlidersPayload { contrast: 60.0, ..Default::default() }),
        ("inverted", SlidersPayload { invert: true, ..Default::default() }),
        (
            "desaturated and warmed",
            SlidersPayload {
                saturation: -60.0,
                wb_temp: 3200.0,
                ..Default::default()
            },
        ),
        (
            "gamma split",
            SlidersPayload {
                red_gamma: 1.8,
                blue_gamma: 0.7,
                ..Default::default()
            },
        ),
    ];

    for (label, sliders) in cases {
        let bins = run_histogram(&img, &sliders);
        let developed = develop(&img, &sliders);

        for channel in 0..3 {
            let gpu_mean = weighted_mean_bin(&bins[channel]);
            let cpu_mean = developed
                .iter()
                .skip(channel)
                .step_by(3)
                .map(|&v| v as f64)
                .sum::<f64>()
                / (64.0 * 64.0);

            assert!(
                (gpu_mean - cpu_mean).abs() < 1.5,
                "{label}, channel {channel}: histogram mean {gpu_mean:.2} but \
                 developed image mean {cpu_mean:.2} — the two chains disagree"
            );
        }
    }
}
