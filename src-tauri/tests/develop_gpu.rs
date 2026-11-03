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
    run_histogram, run_histogram_with, run_histograms, solid, through_linear, u8_to_u16, weighted_mean_bin,
    HIST_VOTE,
};

use open_darkroom_lib::export_rendering::{render_image, SlidersPayload};
use open_darkroom_lib::output_space::OutputSpace;
use open_darkroom_lib::image_opening::{Rgba16Image, HIST_BINS};
use open_darkroom_lib::icc::{Matrix3, SourceSpace, SRGB_TO_WORKING};

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

    // `wb_matrix` carries the source → working-space conversion composed ahead
    // of the white balance, so at the neutral reference temperature it must be
    // exactly the source matrix — the adaptation cancels and leaves nothing of
    // its own behind. Asserting the identity here would only pass if the
    // conversion had been dropped; asserting the source matrix catches both a
    // stray tint and a conversion that never happened.
    let wb = p.wb_matrix();
    for col in 0..3 {
        for row in 0..3 {
            let want = SRGB_TO_WORKING[col * 3 + row];
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
        red_black_point: 40.0,
        red_white_point: 130.0,
        ..Default::default()
    });

    // Column-major: lane 0 is the red scale, lane 12 the red offset.
    assert!(normal.dr_matrix()[0] > 0.0, "normal slope is positive");
    assert!(inverted.dr_matrix()[0] < 0.0, "inverted slope is negative");
    assert_eq!(normal.dr_density(), 0.0, "a positive is remapped in linear light");
    assert_eq!(inverted.dr_density(), 1.0, "a negative is remapped in density");

    // Inverted, the matrix acts on log2 transmittance: the black point must
    // land on white and the white point on black.
    let red = |slider: f64| {
        let density = common::srgb_to_linear(slider / 255.0).log2() as f32;
        inverted.dr_matrix()[0] * density + inverted.dr_matrix()[12]
    };
    assert!((red(40.0) - 1.0).abs() < 1e-4, "black point should land on 1, got {}", red(40.0));
    assert!(red(130.0).abs() < 1e-4, "white point should land on 0, got {}", red(130.0));
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

/// A negative's density is linear in scene stops, so tones spaced evenly in
/// log transmittance between the points must come out evenly spaced in sRGB,
/// with their geometric middle on mid-grey.
#[test]
fn inversion_spaces_negative_stops_evenly() {
    const BLACK: f64 = 40.0;
    const WHITE: f64 = 130.0;
    const STEPS: u32 = 9;
    let sliders = SlidersPayload {
        invert: true,
        red_black_point: BLACK as f32,
        green_black_point: BLACK as f32,
        blue_black_point: BLACK as f32,
        red_white_point: WHITE as f32,
        green_white_point: WHITE as f32,
        blue_white_point: WHITE as f32,
        ..Default::default()
    };

    let lo = common::srgb_to_linear(BLACK / 255.0).log2();
    let hi = common::srgb_to_linear(WHITE / 255.0).log2();
    let img = Rgba16Image::from_fn(STEPS, 1, |x, _| {
        let t = (lo + (hi - lo) * x as f64 / (STEPS - 1) as f64).exp2();
        let v = (common::linear_to_srgb(t) * 65535.0).round() as u16;
        image::Rgba([v, v, v, u16::MAX])
    });
    let out = develop16(&img, &sliders);

    for x in 0..STEPS as usize {
        let expected = 1.0 - x as f64 / (STEPS - 1) as f64;
        let actual = out[x * 3] as f64 / 65535.0;
        assert!(
            (actual - expected).abs() < 2e-3,
            "step {x}: expected sRGB {expected:.4}, got {actual:.4}"
        );
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

/// On a negative, a channel denser than its black point is white for that
/// channel, however far past it the noise floor reaches. Over-range values
/// would otherwise leak into the other channels through the source matrix.
#[test]
fn inversion_holds_tones_past_the_black_point_at_white() {
    let sliders = SlidersPayload {
        invert: true,
        red_black_point: 37.0,
        green_black_point: 15.5,
        blue_black_point: 4.0,
        red_white_point: 164.1,
        green_white_point: 117.6,
        blue_white_point: 94.2,
        ..Default::default()
    };
    let at_black = |slider: f64| (common::srgb_to_linear(slider / 255.0) * 65535.0).round() as u16;
    let highlight = [at_black(37.0), at_black(15.5), at_black(4.0), u16::MAX];
    let noise_floor = [highlight[0], highlight[1], 0, u16::MAX];
    let img = Rgba16Image::from_fn(2, 1, |x, _| image::Rgba(if x == 0 { highlight } else { noise_floor }));

    let space = SourceSpace::linear_with_matrix(CAMERA_LIKE);
    let out = pollster::block_on(render_image(img.as_raw(), 2, 1, 4, &sliders, &space, OutputSpace::Srgb, 8, |_| {}))
        .expect("render should succeed");

    assert!(out[..3].iter().all(|&c| c > 250), "the black points should develop to white, got {:?}", &out[..3]);
    assert!(
        out[3..6].iter().all(|&c| c > 250),
        "blue past its black point should stay white, got {:?}",
        &out[3..6]
    );
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

// ── histogram.wgsl, master input bins ─────────────────────────────────────────

/// A strongly non-diagonal source matrix with rows summing to one, the shape a
/// camera's colour matrix takes. Column-major.
const CAMERA_LIKE: Matrix3 = [1.6, -0.15, 0.0, -0.4, 1.55, -0.4, -0.2, -0.4, 1.4];

/// The master input bins stop before the source matrix mixes the channels, so
/// one channel's sliders move only that channel's bins.
#[test]
fn the_master_input_bins_keep_the_channels_apart() {
    let space = SourceSpace::linear_with_matrix(CAMERA_LIKE);
    let img = rgb_ramp(64, 64);
    let moved = SlidersPayload { green_black_point: 60.0, green_gamma: 1.4, ..Default::default() };

    let (_, before) = run_histograms(&img, &SlidersPayload::default(), &space, true);
    let (_, after) = run_histograms(&img, &moved, &space, true);
    assert_ne!(before[1], after[1], "the green sliders did not move green");
    assert_eq!(before[0], after[0], "the green sliders moved red");
    assert_eq!(before[2], after[2], "the green sliders moved blue");

    // On screen the matrix does carry green into red, which is the point of
    // measuring before it.
    let before = run_histogram_with(&img, &SlidersPayload::default(), &space);
    let after = run_histogram_with(&img, &moved, &space);
    assert_ne!(before[0], after[0], "the fixture no longer exercises the matrix");
}

/// The master input bins sit on the sliders' scale: a patch at slider value `v`
/// lands in bin `v`, and a black point set on it sends it to bin 0.
#[test]
fn the_master_input_bins_read_on_the_slider_scale() {
    for value in [32u8, 128, 200] {
        let img = solid(16, 16, [u8_to_u16(value); 4]);
        let space = SourceSpace::default();

        let (_, bins) = run_histograms(&img, &SlidersPayload::default(), &space, true);
        let mean = weighted_mean_bin(&bins[0]);
        assert!((mean - value as f64).abs() < 1.0, "patch {value} measured at bin {mean:.2}");

        let black = SlidersPayload { red_black_point: value as f32, ..Default::default() };
        let (_, bins) = run_histograms(&img, &black, &space, true);
        assert!(weighted_mean_bin(&bins[0]) < 0.5, "black point {value} did not reach bin 0");
    }
}

// ── Master input levels ───────────────────────────────────────────────────────

/// With its black and white points left alone, the master gamma is the same as
/// multiplying every channel's gamma by it.
#[test]
fn the_master_gamma_composes_with_the_channel_gammas() {
    let img = rgb_ramp(64, 4);
    let master = SlidersPayload { red_gamma: 1.2, master_gamma: 1.5, ..Default::default() };
    let channels = SlidersPayload {
        red_gamma: 1.8,
        green_gamma: 1.5,
        blue_gamma: 1.5,
        ..Default::default()
    };

    let a = develop(&img, &master);
    let b = develop(&img, &channels);
    let worst = a.iter().zip(&b).map(|(&x, &y)| (x as i32 - y as i32).abs()).max().unwrap();
    assert!(worst <= 1, "master gamma differed from the composed channel gamma by {worst}");
}

/// The master points sit on the sliders' scale and act after inversion, so on
/// the positive black is black and white is white.
#[test]
fn the_master_points_act_on_the_positive() {
    let space = SourceSpace::default();
    let black_out = develop(&solid(16, 16, [0, 0, 0, u16::MAX]), &SlidersPayload::default());
    let white_out = develop(&solid(16, 16, [u16::MAX; 4]), &SlidersPayload::default());
    for value in [32u8, 128, 200] {
        let img = solid(16, 16, [u8_to_u16(value); 4]);

        let black = SlidersPayload { master_black_point: value as f32, ..Default::default() };
        assert_eq!(develop(&img, &black), black_out, "master black {value} did not reach black");

        let white = SlidersPayload { master_white_point: value as f32, ..Default::default() };
        assert_eq!(develop(&img, &white), white_out, "master white {value} did not reach white");

        // Inverted, a master black point set just above where the patch lands
        // still sends it to black.
        let inverted = SlidersPayload { invert: true, ..Default::default() };
        let landed = weighted_mean_bin(&run_histograms(&img, &inverted, &space, true).1[1]);
        let inverted = SlidersPayload { master_black_point: landed as f32 + 1.0, ..inverted };
        assert_eq!(develop(&img, &inverted), black_out, "inverted patch {value} was not sent to black");
    }
}

/// The master slider's bins are taken just ahead of it: they follow the channel
/// sliders but hold still while the master's own handles move.
#[test]
fn the_master_histogram_is_measured_at_its_input() {
    let space = SourceSpace::default();
    let img = rgb_ramp(64, 64);
    let base = SlidersPayload::default();
    let mastered = SlidersPayload {
        master_black_point: 40.0,
        master_white_point: 220.0,
        master_gamma: 1.6,
        ..Default::default()
    };
    let channel = SlidersPayload { green_black_point: 60.0, ..Default::default() };

    let (main_before, master_before) = run_histograms(&img, &base, &space, true);
    let (main_after, master_after) = run_histograms(&img, &mastered, &space, true);
    assert_eq!(master_before, master_after, "the master sliders moved their own histogram");
    assert_ne!(main_before, main_after, "the master sliders did not move the main histogram");

    let (_, master_moved) = run_histograms(&img, &channel, &space, true);
    assert_ne!(master_before[1], master_moved[1], "the green sliders did not reach the master histogram");

    // Measuring the master input changes nothing about the main bins, and
    // leaving it off leaves the master bins empty.
    let (main_output, master_output) = run_histograms(&img, &base, &space, false);
    assert_eq!(main_before, main_output, "measuring the master input moved the main histogram");
    assert!(master_output.iter().flatten().all(|&v| v == 0), "the master bins were written unasked");
}
