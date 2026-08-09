// src-tauri/tests/sliders_gpu.rs
//
// Layer 2, per-slider: one synthetic probe per control the UI exposes, asking
// "does moving this slider produce the output a photographer would expect?".
//
// `develop_gpu.rs` covers the chain as a whole — identity, inversion, the
// histogram agreeing with the canvas. This file covers each of the 24 sliders
// individually: the right direction, the right magnitude where there is a
// reference to compare against, and no leakage into channels or tonal regions
// the slider has no business touching.
//
// Reference values are computed in f64 on the CPU from the documented intent of
// each control, never by re-running the shader, so a test failure means the
// shader disagrees with the specification rather than with itself.

mod common;

use common::{
    develop, develop_patch, develop_patches, gray_ramp, linear_to_srgb, mixed_ramp, srgb_to_linear,
    through_linear, u8_to_u16,
};

use image::{ImageBuffer, Rgba};
use open_darkroom_lib::export_rendering::SlidersPayload;
use open_darkroom_lib::image_opening::Rgba16Image;

/// Every comparison against a CPU reference allows this much 8-bit slack.
/// One code value is texture-format rounding (the preview is Rgba16Float); two
/// leaves room for vendor differences in `pow`.
const TOL: i32 = 2;

fn close(actual: u8, expected: u8) -> bool {
    (actual as i32 - expected as i32).abs() <= TOL
}

/// True when the two patches differ by more than rounding noise.
fn differs(a: [u8; 3], b: [u8; 3]) -> bool {
    (0..3).any(|c| (a[c] as i32 - b[c] as i32).abs() > TOL)
}

fn defaults() -> SlidersPayload {
    SlidersPayload::default()
}

/// The contrast curve of `develop.wgsl`, recomputed in f64 from its stated
/// definition rather than by re-running the shader.
///
/// At default sliders every earlier step is the identity, so the encoded tone
/// the shader sees is exactly `input / 255`.
fn contrast_reference(input: u8, contrast: f64) -> u8 {
    const PIVOT: f64 = 0.5;
    const BASE: f64 = 3.0;

    let k = BASE.powf(contrast / 100.0);
    let x = input as f64 / 255.0;

    let out = if x <= 0.0 || x >= 1.0 {
        x
    } else if x < PIVOT {
        PIVOT * (x / PIVOT).powf(k)
    } else {
        1.0 - (1.0 - PIVOT) * ((1.0 - x) / (1.0 - PIVOT)).powf(k)
    };

    (out * 255.0).round() as u8
}

/// Strict monotonicity check over a ramp developed with `sliders`.
fn assert_tone_order(label: &str, steps: usize, sliders: &SlidersPayload) {
    let probes: Vec<u8> = (0..steps)
        .map(|i| (i * 255 / (steps - 1)) as u8)
        .collect();
    let out = develop_patches(&probes, sliders);

    for i in 1..probes.len() {
        assert!(
            out[i][0] >= out[i - 1][0],
            "{label}: input {} → {} but input {} → {} (tone order reversed)",
            probes[i - 1],
            out[i - 1][0],
            probes[i],
            out[i][0]
        );
    }
}

// ── Darkroom group: black points ──────────────────────────────────────────────

/// The black point is the Hasselblad-style control the whole app is built
/// around: it must remap in *linear* light on its own channel only.
#[test]
fn each_black_point_lifts_only_its_own_channel() {
    /// Where the darkroom matrix should land an input, per calcParams.wgsl.
    fn expected(input: u8, black: f64, white: f64) -> u8 {
        let lin = srgb_to_linear(input as f64 / 255.0);
        let bp = srgb_to_linear(black / 255.0);
        let wp = srgb_to_linear(white / 255.0);
        let slope = 1.0 / (wp - bp);
        let out = lin * slope - bp * slope;
        (linear_to_srgb(out.clamp(0.0, 1.0)) * 255.0).round() as u8
    }

    let want = expected(128, 64.0, 255.0);

    for (channel, name) in [(0usize, "red"), (1, "green"), (2, "blue")] {
        let mut sliders = defaults();
        match channel {
            0 => sliders.red_black_point = 64.0,
            1 => sliders.green_black_point = 64.0,
            _ => sliders.blue_black_point = 64.0,
        }

        let got = develop_patch([128; 3], &sliders);

        assert!(
            close(got[channel], want),
            "{name} black point 64 on input 128: expected ~{want}, got {} ({got:?})",
            got[channel]
        );
        for other in (0..3).filter(|&c| c != channel) {
            assert!(
                close(got[other], 128),
                "{name} black point leaked into channel {other}: {got:?}"
            );
        }
    }
}

#[test]
fn a_black_point_clips_everything_below_it() {
    let sliders = SlidersPayload {
        red_black_point: 100.0,
        green_black_point: 100.0,
        blue_black_point: 100.0,
        ..defaults()
    };

    for value in [0u8, 40, 99] {
        let got = develop_patch([value; 3], &sliders);
        assert!(
            got.iter().all(|&c| c < 5),
            "input {value} sits below the black point and should crush to black, got {got:?}"
        );
    }
    assert!(
        develop_patch([160; 3], &sliders).iter().all(|&c| c > 5),
        "input above the black point must survive"
    );
}

// ── Darkroom group: white points ──────────────────────────────────────────────

#[test]
fn each_white_point_pulls_only_its_own_channel() {
    for (channel, name) in [(0usize, "red"), (1, "green"), (2, "blue")] {
        let mut sliders = defaults();
        match channel {
            0 => sliders.red_white_point = 200.0,
            1 => sliders.green_white_point = 200.0,
            _ => sliders.blue_white_point = 200.0,
        }

        let got = develop_patch([200; 3], &sliders);

        assert!(
            got[channel] > 250,
            "{name} white point 200 should drive input 200 to white, got {}",
            got[channel]
        );
        for other in (0..3).filter(|&c| c != channel) {
            assert!(
                close(got[other], 200),
                "{name} white point leaked into channel {other}: {got:?}"
            );
        }
    }
}

// ── Darkroom group: output black and white ────────────────────────────────────

/// The output pair compresses the whole tonal range into a narrower window —
/// the "print" end of the darkroom controls. Both ends must land exactly, and
/// the interpolation between them happens in linear light.
#[test]
fn the_output_range_maps_the_endpoints_exactly() {
    let sliders = SlidersPayload {
        rgb_output_black: 64.0,
        rgb_output_white: 192.0,
        ..defaults()
    };

    let black = develop_patch([0; 3], &sliders);
    let white = develop_patch([255; 3], &sliders);
    let mid = develop_patch([128; 3], &sliders);

    assert!(
        black.iter().all(|&c| close(c, 64)),
        "input black should land on the output black point, got {black:?}"
    );
    assert!(
        white.iter().all(|&c| close(c, 192)),
        "input white should land on the output white point, got {white:?}"
    );

    // Linear interpolation between the two output points, in linear light.
    let ob = srgb_to_linear(64.0 / 255.0);
    let ow = srgb_to_linear(192.0 / 255.0);
    let want =
        (linear_to_srgb(srgb_to_linear(128.0 / 255.0) * (ow - ob) + ob) * 255.0).round() as u8;
    assert!(
        mid.iter().all(|&c| close(c, want)),
        "mid grey should interpolate to ~{want} in linear light, got {mid:?}"
    );
}

#[test]
fn raising_the_output_black_lifts_the_shadows_off_zero() {
    let sliders = SlidersPayload {
        rgb_output_black: 80.0,
        ..defaults()
    };
    let black = develop_patch([0; 3], &sliders);
    assert!(
        black.iter().all(|&c| close(c, 80)),
        "output black 80 should lift pure black to ~80, got {black:?}"
    );
}

/// Gamma sits *between* the input remap and the output remap, so the output pair
/// pins the endpoints whatever gamma does — the order a Levels dialog and a
/// scanner both use. With the output range folded into the darkroom matrix it
/// was applied before the power instead, and gamma dragged both ends away:
/// output black 80 landed at 153 and output white 200 at 228.
#[test]
fn the_output_range_holds_its_endpoints_under_gamma() {
    let sliders = SlidersPayload {
        rgb_output_black: 80.0,
        rgb_output_white: 200.0,
        red_gamma: 2.2,
        green_gamma: 2.2,
        blue_gamma: 2.2,
        ..defaults()
    };

    let black = develop_patch([0; 3], &sliders);
    let white = develop_patch([255; 3], &sliders);

    assert!(
        black.iter().all(|&c| close(c, 80)),
        "output black must survive gamma 2.2, got {black:?}"
    );
    assert!(
        white.iter().all(|&c| close(c, 200)),
        "output white must survive gamma 2.2, got {white:?}"
    );
}

/// The same guarantee on the path that actually matters: a negative being
/// inverted, with gamma shaping the positive and the output pair setting the
/// print's black and white.
#[test]
fn the_output_range_holds_its_endpoints_when_inverted_under_gamma() {
    let sliders = SlidersPayload {
        invert: true,
        rgb_output_black: 80.0,
        rgb_output_white: 200.0,
        red_gamma: 2.2,
        green_gamma: 2.2,
        blue_gamma: 2.2,
        ..defaults()
    };

    // Inverted, so full input maps to the output black and vice versa.
    let from_white = develop_patch([255; 3], &sliders);
    let from_black = develop_patch([0; 3], &sliders);

    assert!(
        from_white.iter().all(|&c| close(c, 80)),
        "inverted, input white must land on output black, got {from_white:?}"
    );
    assert!(
        from_black.iter().all(|&c| close(c, 200)),
        "inverted, input black must land on output white, got {from_black:?}"
    );
}

// ── Darkroom group: per-channel gamma ─────────────────────────────────────────

#[test]
fn each_gamma_lifts_only_its_own_channel() {
    let want = through_linear(128, |l| l.powf(1.0 / 2.2));

    for (channel, name) in [(0usize, "red"), (1, "green"), (2, "blue")] {
        let mut sliders = defaults();
        match channel {
            0 => sliders.red_gamma = 2.2,
            1 => sliders.green_gamma = 2.2,
            _ => sliders.blue_gamma = 2.2,
        }

        let got = develop_patch([128; 3], &sliders);

        assert!(
            close(got[channel], want),
            "{name} gamma 2.2 on input 128: expected ~{want}, got {} ({got:?})",
            got[channel]
        );
        for other in (0..3).filter(|&c| c != channel) {
            assert!(
                close(got[other], 128),
                "{name} gamma leaked into channel {other}: {got:?}"
            );
        }
    }
}

/// Gamma below 1 must darken, not brighten — the sign of the exponent is easy
/// to invert and an all-gamma-above-1 test would never notice.
#[test]
fn gamma_below_one_darkens_the_midtones() {
    let dark = develop_patch(
        [128; 3],
        &SlidersPayload {
            red_gamma: 0.5,
            green_gamma: 0.5,
            blue_gamma: 0.5,
            ..defaults()
        },
    );
    assert!(
        dark.iter().all(|&c| c < 120),
        "gamma 0.5 should darken mid grey, got {dark:?}"
    );
}

// ── White balance ─────────────────────────────────────────────────────────────

/// The temperature slider names the illuminant the scene is assumed to have had,
/// so raising it warms the image — the Lightroom convention. Getting this
/// backwards is the single most visible possible error in the panel.
#[test]
fn raising_the_temperature_warms_the_image() {
    let cool = develop_patch(
        [160; 3],
        &SlidersPayload {
            wb_temp: 3200.0,
            ..defaults()
        },
    );
    let warm = develop_patch(
        [160; 3],
        &SlidersPayload {
            wb_temp: 8800.0,
            ..defaults()
        },
    );

    assert!(
        cool[2] > cool[1] && cool[1] > cool[0],
        "3200 K should render a neutral as blue (B > G > R), got {cool:?}"
    );
    assert!(
        warm[0] > warm[1] && warm[1] > warm[2],
        "8800 K should render a neutral as warm (R > G > B), got {warm:?}"
    );
}

#[test]
fn the_red_to_blue_ratio_rises_with_temperature() {
    let mut previous = f64::NEG_INFINITY;
    for temp in [2200.0f32, 3200.0, 5500.0, 6500.0, 8800.0] {
        let got = develop_patch(
            [160; 3],
            &SlidersPayload {
                wb_temp: temp,
                ..defaults()
            },
        );
        let ratio = got[0] as f64 / (got[2] as f64 + 1.0);
        assert!(
            ratio > previous,
            "{temp} K gave R/B {ratio:.3}, not warmer than the step below ({previous:.3})"
        );
        previous = ratio;
    }
}

/// Tint is the magenta/green axis: green is scaled by `2^(-tint/100)`, then the
/// whole matrix is renormalised so the neutral axis keeps its luminance. Red and
/// blue therefore move too — by the normalisation factor alone, together.
#[test]
fn tint_moves_green_against_the_slider() {
    for (tint, label) in [(50.0f32, "magenta"), (-50.0, "green")] {
        let got = develop_patch(
            [160; 3],
            &SlidersPayload {
                wb_tint: tint,
                ..defaults()
            },
        );

        let factor = 2f64.powf(-tint as f64 / 100.0);
        // luma of the transformed white, which is what calcParams divides by.
        let norm = 1.0 / (0.2126 + 0.7152 * factor + 0.0722);
        let lin = srgb_to_linear(160.0 / 255.0);
        let encode =
            |l: f64| (linear_to_srgb(l.clamp(0.0, 1.0)) * 255.0).round() as u8;

        let want_green = encode(lin * factor * norm);
        let want_other = encode(lin * norm);

        assert!(
            close(got[1], want_green),
            "tint {tint} ({label}) should put green at ~{want_green}, got {} ({got:?})",
            got[1]
        );
        assert!(
            close(got[0], want_other) && close(got[2], want_other),
            "tint {tint} should move red and blue together to ~{want_other}, got {got:?}"
        );
        // The cast itself: positive tint pushes green down relative to the rest.
        if tint > 0.0 {
            assert!(got[1] < got[0], "positive tint is magenta, got {got:?}");
        } else {
            assert!(got[1] > got[0], "negative tint is green, got {got:?}");
        }
    }
}

/// White balance is a colour control, not a hidden exposure control.
///
/// Before the matrix was renormalised, tint +100 cost two thirds of a stop and
/// tint -100 gained three quarters of one, so correcting a cast forced a second
/// correction to the exposure — and the histogram never settled. This is the
/// test that keeps the two decisions independent.
#[test]
fn white_balance_does_not_change_the_exposure() {
    /// BT.709 luminance of a developed patch, in linear light.
    fn luma(p: [u8; 3]) -> f64 {
        [0.2126, 0.7152, 0.0722]
            .iter()
            .zip(p.iter())
            .map(|(w, &c)| w * srgb_to_linear(c as f64 / 255.0))
            .sum()
    }

    // Both ends of the UI's tint range, and the temperature range the slider
    // offers apart from the bottom rail (see the out-of-gamut test below).
    let temps = [3200.0f32, 4300.0, 5500.0, 6500.0, 7500.0, 8800.0];
    let tints = [-100.0f32, -50.0, 0.0, 50.0, 100.0];

    // 4% of the input luminance. One 8-bit code value at these levels is already
    // ~1%, and the defect this guards against was 60-75%.
    const BUDGET: f64 = 0.04;

    // The guarantee is about the matrix, so it can only be asserted where the
    // result survives the 0..1 clamp: a channel pinned at a rail has lost light
    // to the output gamut, which no normalisation can give back. Clipped samples
    // are skipped and counted, and the count is checked at the end so the test
    // cannot pass by skipping everything.
    let mut checked = 0;
    let mut clipped = 0;

    for &value in [64u8, 96, 128].iter() {
        let reference = srgb_to_linear(value as f64 / 255.0);
        for &wb_temp in temps.iter() {
            for &wb_tint in tints.iter() {
                let got = develop_patch(
                    [value; 3],
                    &SlidersPayload {
                        wb_temp,
                        wb_tint,
                        ..defaults()
                    },
                );

                if got.iter().any(|&c| c == 0 || c == 255) {
                    clipped += 1;
                    continue;
                }

                let drift = luma(got) / reference - 1.0;
                assert!(
                    drift.abs() < BUDGET,
                    "neutral {value} at {wb_temp} K / tint {wb_tint} changed luminance by \
                     {:+.1}% ({got:?}) — white balance is acting as an exposure control",
                    drift * 100.0
                );
                checked += 1;
            }
        }
    }

    let total = 3 * temps.len() * tints.len();
    assert!(
        checked * 4 >= total * 3,
        "only {checked} of {total} combinations were in gamut ({clipped} clipped) — \
         too few to call this a guarantee"
    );
}

/// Normalising for luminance must not flatten the control into doing nothing:
/// the channel ratios — the actual colour decision — have to survive it.
#[test]
fn white_balance_still_shifts_the_colour() {
    let neutral = develop_patch([160; 3], &defaults());

    for (label, sliders) in [
        ("3200 K", SlidersPayload { wb_temp: 3200.0, ..defaults() }),
        ("8800 K", SlidersPayload { wb_temp: 8800.0, ..defaults() }),
        ("tint +60", SlidersPayload { wb_tint: 60.0, ..defaults() }),
        ("tint -60", SlidersPayload { wb_tint: -60.0, ..defaults() }),
    ] {
        let got = develop_patch([160; 3], &sliders);
        let spread = *got.iter().max().unwrap() as i32 - *got.iter().min().unwrap() as i32;
        assert!(
            spread > 15,
            "{label} left the neutral almost untinted ({got:?} vs {neutral:?}) — \
             the normalisation has swallowed the control"
        );
    }
}

/// Below about 2360 K, adapting a neutral all the way to the 5500 K reference
/// lands outside sRGB: red goes negative and clips. That is a gamut limit of the
/// output space, not a bug in the matrix, and Lightroom clips there too. The UI
/// rail stops at 2700 K, on the safe side of it, but presets and export can pass
/// anything, so the boundary is pinned rather than left to be rediscovered.
#[test]
fn the_coldest_temperatures_clip_red_out_of_gamut() {
    let got = develop_patch(
        [160; 3],
        &SlidersPayload {
            wb_temp: 2200.0,
            ..defaults()
        },
    );
    assert_eq!(got[0], 0, "2200 K should drive red out of gamut, got {got:?}");
    assert!(got[2] > 240, "...while blue saturates, got {got:?}");

    // And the clip must not spread: at 3200 K everything is still in range.
    let ok = develop_patch(
        [160; 3],
        &SlidersPayload {
            wb_temp: 3200.0,
            ..defaults()
        },
    );
    assert!(
        ok.iter().all(|&c| c > 0 && c < 255),
        "3200 K should stay inside the gamut, got {ok:?}"
    );
}

// ── Exposure ──────────────────────────────────────────────────────────────────

/// Exposure is measured in stops, so it must be a power of two in linear light
/// and symmetric about zero.
#[test]
fn exposure_is_symmetric_in_stops() {
    for (stops, factor) in [(1.0f32, 2.0f64), (-1.0, 0.5), (2.0, 4.0), (-2.0, 0.25)] {
        let sliders = SlidersPayload {
            exposure: stops,
            ..defaults()
        };
        for value in [40u8, 90, 160] {
            let got = develop_patch([value; 3], &sliders);
            let want = through_linear(value, |l| l * factor);
            assert!(
                got.iter().all(|&c| close(c, want)),
                "{stops:+} EV on input {value}: expected ~{want}, got {got:?}"
            );
        }
    }
}

// ── Contrast ──────────────────────────────────────────────────────────────────

/// The curve is a closed form, so this is a true oracle rather than a direction
/// check: every probe is compared against an independent f64 evaluation.
#[test]
fn contrast_matches_the_reference_curve() {
    let probes: Vec<u8> = (0..17).map(|i| (i * 255 / 16) as u8).collect();

    for contrast in [-100.0f32, -60.0, -25.0, 25.0, 60.0, 100.0] {
        let out = develop_patches(
            &probes,
            &SlidersPayload {
                contrast,
                ..defaults()
            },
        );
        for (i, &input) in probes.iter().enumerate() {
            let want = contrast_reference(input, contrast as f64);
            assert!(
                close(out[i][0], want),
                "contrast {contrast} on input {input}: expected ~{want}, got {}",
                out[i][0]
            );
        }
    }
}

/// Three properties a Lightroom user relies on without thinking about them:
/// mid grey does not drift, black stays black, and white stays white — at every
/// slider position, including the rails.
#[test]
fn contrast_holds_mid_grey_and_both_endpoints() {
    for contrast in [-100.0f32, -50.0, -10.0, 10.0, 50.0, 100.0] {
        let got = develop_patches(
            &[0u8, 128, 255],
            &SlidersPayload {
                contrast,
                ..defaults()
            },
        );

        assert!(
            close(got[1][0], 128),
            "contrast {contrast} moved mid grey to {} — it should be the pivot",
            got[1][0]
        );
        assert!(
            got[0][0] <= TOL as u8,
            "contrast {contrast} lifted black to {}",
            got[0][0]
        );
        assert!(
            got[2][0] >= 255 - TOL as u8,
            "contrast {contrast} pulled white down to {} — at +100 the old sigmoid \
             could only reach 254",
            got[2][0]
        );
    }
}

/// The defect that used to live here: `mix(rgb, sigmoid, contrast)` ran the tone
/// curve backwards below contrast -67, folding smooth gradients. The power curve
/// is monotone for every exponent, so this now holds at every setting.
#[test]
fn contrast_keeps_the_tone_order_at_every_setting() {
    for contrast in [-100.0f32, -85.0, -70.0, -40.0, 40.0, 70.0, 100.0] {
        assert_tone_order(
            &format!("contrast {contrast}"),
            33,
            &SlidersPayload {
                contrast,
                ..defaults()
            },
        );
    }
}

/// Because `k = 3^contrast`, the negative branch is the exact inverse of the
/// positive one — so developing at +50 and then at -50 returns the original
/// tone. Nothing else in the chain has this property, and it is what makes the
/// slider feel like it is undoing itself as you drag back through zero.
#[test]
fn negative_contrast_inverts_positive_contrast() {
    let probes: Vec<u8> = (1..16).map(|i| (i * 255 / 16) as u8).collect();

    for magnitude in [30.0f32, 60.0] {
        let up = develop_patches(
            &probes,
            &SlidersPayload {
                contrast: magnitude,
                ..defaults()
            },
        );

        // Feed each developed tone back through the opposite setting.
        let intermediate: Vec<u8> = up.iter().map(|p| p[0]).collect();
        let back = develop_patches(
            &intermediate,
            &SlidersPayload {
                contrast: -magnitude,
                ..defaults()
            },
        );

        for (i, &input) in probes.iter().enumerate() {
            // 8-bit round trip through a steepened curve loses precision where
            // the curve is flattest, so this is looser than TOL by design.
            assert!(
                (back[i][0] as i32 - input as i32).abs() <= 6,
                "contrast +{magnitude} then -{magnitude} on input {input} gave {}",
                back[i][0]
            );
        }
    }
}

/// Positive contrast must pull the shadows down and the highlights up while
/// leaving mid grey roughly where it is.
#[test]
fn positive_contrast_widens_the_tonal_spread() {
    let probes = [64u8, 128, 192];
    let base = develop_patches(&probes, &defaults());
    let hot = develop_patches(
        &probes,
        &SlidersPayload {
            contrast: 50.0,
            ..defaults()
        },
    );

    let spread = |p: &[[u8; 3]]| p[2][0] as i32 - p[0][0] as i32;
    assert!(
        spread(&hot) > spread(&base) + 5,
        "contrast +50 should widen the 64→192 spread: {} → {}",
        spread(&base),
        spread(&hot)
    );
    assert!(
        hot[0][0] < base[0][0],
        "contrast should darken the shadow probe: {} → {}",
        base[0][0],
        hot[0][0]
    );
    assert!(
        hot[2][0] > base[2][0],
        "contrast should brighten the highlight probe: {} → {}",
        base[2][0],
        hot[2][0]
    );
}

#[test]
fn negative_contrast_narrows_the_tonal_spread() {
    let probes = [64u8, 192];
    let base = develop_patches(&probes, &defaults());
    let flat = develop_patches(
        &probes,
        &SlidersPayload {
            contrast: -50.0,
            ..defaults()
        },
    );

    let spread = |p: &[[u8; 3]]| p[1][0] as i32 - p[0][0] as i32;
    assert!(
        spread(&flat) < spread(&base) - 5,
        "contrast -50 should narrow the 64→192 spread: {} → {}",
        spread(&base),
        spread(&flat)
    );
}

// ── Brightness ────────────────────────────────────────────────────────────────

/// Brightness is a uniform shift in encoded code values, so the reference is
/// exact: `slider/100 × 0.25` of full scale, added to every tone equally.
///
/// It used to be an additive offset in *linear* light on a -1..1 range, which
/// meant +0.2 took sRGB 32 all the way to 128 while barely touching a highlight.
#[test]
fn brightness_shifts_every_tone_by_the_same_amount() {
    const MAX_BRIGHTNESS: f64 = 0.25;

    let probes = [32u8, 90, 160, 220];
    let base = develop_patches(&probes, &defaults());

    for slider in [-50.0f32, -20.0, 20.0, 50.0] {
        let got = develop_patches(
            &probes,
            &SlidersPayload {
                brightness: slider,
                ..defaults()
            },
        );
        let shift = slider as f64 / 100.0 * MAX_BRIGHTNESS * 255.0;

        for (i, &value) in probes.iter().enumerate() {
            let want = (value as f64 + shift).clamp(0.0, 255.0).round() as u8;
            assert!(
                close(got[i][0], want),
                "brightness {slider} on input {value}: expected ~{want}, got {} \
                 (a uniform shift of {shift:+.1} code values)",
                got[i][0]
            );
            assert!(
                got[i][0] == got[i][1] && got[i][1] == got[i][2],
                "brightness must stay neutral, got {:?}",
                got[i]
            );
        }

        // Direction, stated separately so a sign flip cannot hide behind the
        // magnitude check above.
        let expect_up = slider > 0.0;
        for i in 0..probes.len() {
            assert_eq!(
                got[i][0] > base[i][0],
                expect_up,
                "brightness {slider} moved input {} the wrong way: {} → {}",
                probes[i],
                base[i][0],
                got[i][0]
            );
        }
    }
}

/// A pure offset is monotone at any amplitude, but the ramp is checked anyway
/// because brightness now shares the encoded tone block with the controls that
/// are not.
#[test]
fn brightness_keeps_the_tone_order_at_every_setting() {
    for brightness in [-100.0f32, -40.0, 40.0, 100.0] {
        assert_tone_order(
            &format!("brightness {brightness}"),
            33,
            &SlidersPayload {
                brightness,
                ..defaults()
            },
        );
    }
}

// ── Highlights / shadows / whites / blacks ────────────────────────────────────
//
// Each of the four is an additive offset gated by a luminance mask, applied to a
// perceptually encoded signal. The masks therefore split the *visible* range
// into quarters — blacks below sRGB 64, shadows below 128, highlights above 128,
// whites above 191 — and those boundaries are what make them four controls
// rather than one. Each test names a tone the slider must move and a tone it must
// not touch at all.
//
// The amplitudes are budgeted in calcParams so the sum of the mask slopes can
// never reach 1; `the_region_controls_keep_the_tone_order_in_combination` is the
// test that holds that budget honest.

/// Amount, in 8-bit code values, that a control must move a tone before the move
/// counts as real rather than as rounding.
const MOVED: i32 = 5;

/// Develop `probes` at defaults and at one region setting, returning both rows.
fn region_rows(
    probes: &[u8],
    set: fn(&mut SlidersPayload, f32),
    amount: f32,
) -> (Vec<u8>, Vec<u8>) {
    let mut sliders = defaults();
    set(&mut sliders, amount);
    (
        develop_patches(probes, &defaults())
            .iter()
            .map(|p| p[0])
            .collect(),
        develop_patches(probes, &sliders).iter().map(|p| p[0]).collect(),
    )
}

#[test]
fn highlights_act_only_above_mid_grey() {
    let probes = [0u8, 40, 90, 128, 170, 245];
    let (base, lifted) = region_rows(&probes, |s, a| s.highlights = a, 100.0);

    // smoothstep(0.5, 1.0, luma) — nothing at or below mid grey.
    for i in 0..4 {
        assert!(
            (lifted[i] as i32 - base[i] as i32).abs() <= TOL,
            "highlights must not touch input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }
    for i in 4..6 {
        assert!(
            lifted[i] as i32 > base[i] as i32 + MOVED,
            "highlights +100 should lift input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }

    // And the other direction pulls them back down.
    let (_, pulled) = region_rows(&probes, |s, a| s.highlights = a, -100.0);
    assert!(
        (pulled[4] as i32) < base[4] as i32 - MOVED,
        "highlights -100 should darken input 170: {} → {}",
        base[4],
        pulled[4]
    );
}

#[test]
fn shadows_act_only_below_mid_grey() {
    let probes = [0u8, 40, 90, 128, 170, 245];
    let (base, lifted) = region_rows(&probes, |s, a| s.shadows = a, 100.0);

    for i in 0..3 {
        assert!(
            lifted[i] as i32 > base[i] as i32 + MOVED,
            "shadows +100 should lift input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }
    for i in 3..6 {
        assert!(
            (lifted[i] as i32 - base[i] as i32).abs() <= TOL,
            "shadows must not touch input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }

    // The mask weights darker tones more heavily, which is what separates
    // "shadows" from a plain brightness lift.
    let deep = lifted[0] as i32 - base[0] as i32;
    let shallow = lifted[2] as i32 - base[2] as i32;
    assert!(
        deep > shallow,
        "the shadow mask should favour the deeper shadow: +{deep} at 0 vs +{shallow} at 90"
    );
}

#[test]
fn whites_act_only_in_the_top_quarter() {
    let probes = [128u8, 170, 210, 245];
    let (base, lifted) = region_rows(&probes, |s, a| s.whites = a, 100.0);

    // smoothstep(0.75, 1.0, luma) — later than the highlight mask, which is the
    // entire reason both controls exist.
    for i in 0..2 {
        assert!(
            (lifted[i] as i32 - base[i] as i32).abs() <= TOL,
            "whites must not touch input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }
    assert!(
        lifted[3] as i32 > base[3] as i32 + MOVED,
        "whites +100 should lift input 245: {} → {}",
        base[3],
        lifted[3]
    );

    // 210 sits at the very start of the mask's ramp, so it moves — but by less
    // than 245 does. That gradient is the mask working; a step instead of a ramp
    // would show as a hard edge in a bright gradient.
    let at_210 = lifted[2] as i32 - base[2] as i32;
    let at_245 = lifted[3] as i32 - base[3] as i32;
    assert!(
        at_210 > 0 && at_210 < at_245,
        "the white mask should ramp up: +{at_210} at 210 vs +{at_245} at 245"
    );
}

#[test]
fn blacks_act_only_in_the_bottom_quarter() {
    let probes = [0u8, 40, 90, 128, 245];
    let (base, lifted) = region_rows(&probes, |s, a| s.blacks = a, 100.0);

    for i in 0..2 {
        assert!(
            lifted[i] as i32 > base[i] as i32 + MOVED,
            "blacks +100 should lift input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }
    // 1 - smoothstep(0, 0.25, luma) — closed by sRGB 64.
    for i in 2..5 {
        assert!(
            (lifted[i] as i32 - base[i] as i32).abs() <= TOL,
            "blacks must not touch input {}: {} → {}",
            probes[i],
            base[i],
            lifted[i]
        );
    }

    // Blacks reaches pure black, which is what gives the faded-print look; the
    // shadows mask does too, but blacks is the narrower of the two.
    assert!(
        lifted[0] as i32 > MOVED,
        "blacks +100 should lift pure black off zero, got {}",
        lifted[0]
    );
}

/// The four masks must not all fire on the same tone, or they are one slider
/// wearing four labels. Each column of this table is a tone; each row is which
/// controls reach it.
#[test]
fn the_four_masks_cover_four_different_ranges() {
    let probes = [20u8, 100, 150, 230];
    let base: Vec<u8> = develop_patches(&probes, &defaults())
        .iter()
        .map(|p| p[0])
        .collect();

    let reach = |set: fn(&mut SlidersPayload, f32)| -> Vec<bool> {
        let (_, row) = region_rows(&probes, set, 100.0);
        (0..probes.len())
            .map(|i| (row[i] as i32 - base[i] as i32).abs() > TOL)
            .collect()
    };

    let blacks = reach(|s, a| s.blacks = a);
    let shadows = reach(|s, a| s.shadows = a);
    let highlights = reach(|s, a| s.highlights = a);
    let whites = reach(|s, a| s.whites = a);

    let all = [
        ("blacks", &blacks),
        ("shadows", &shadows),
        ("highlights", &highlights),
        ("whites", &whites),
    ];
    for (i, (name_a, a)) in all.iter().enumerate() {
        for (name_b, b) in all.iter().skip(i + 1) {
            assert_ne!(
                a, b,
                "{name_a} and {name_b} reach exactly the same tones across {probes:?} \
                 ({a:?}) — the masks are not distinct"
            );
        }
    }

    // The two sides must not cross: nothing on the shadow side may reach the
    // brightest probe, and nothing on the highlight side the darkest.
    assert!(!blacks[3] && !shadows[3], "a shadow control reached input 230");
    assert!(
        !highlights[0] && !whites[0],
        "a highlight control reached input 20"
    );
}

/// The defect that used to live here: the offsets outran their masks and the
/// tone curve doubled back, so lifting the blacks past about a sixth of the
/// slider folded shadow detail over on itself. The masks also overlap, so the
/// budget has to hold for *combinations*, not just for one slider at a time —
/// that is the case a per-slider test would have missed.
#[test]
fn the_region_controls_keep_the_tone_order_in_combination() {
    let cases: Vec<(&str, SlidersPayload)> = vec![
        ("blacks +100", SlidersPayload { blacks: 100.0, ..defaults() }),
        ("blacks -100", SlidersPayload { blacks: -100.0, ..defaults() }),
        ("shadows +100", SlidersPayload { shadows: 100.0, ..defaults() }),
        ("shadows -100", SlidersPayload { shadows: -100.0, ..defaults() }),
        ("highlights +100", SlidersPayload { highlights: 100.0, ..defaults() }),
        ("whites -100", SlidersPayload { whites: -100.0, ..defaults() }),
        (
            "shadows and blacks both +100",
            SlidersPayload { shadows: 100.0, blacks: 100.0, ..defaults() },
        ),
        (
            "shadows and blacks both -100",
            SlidersPayload { shadows: -100.0, blacks: -100.0, ..defaults() },
        ),
        (
            "highlights and whites both -100",
            SlidersPayload { highlights: -100.0, whites: -100.0, ..defaults() },
        ),
        (
            "all four +100",
            SlidersPayload {
                highlights: 100.0,
                shadows: 100.0,
                whites: 100.0,
                blacks: 100.0,
                ..defaults()
            },
        ),
        (
            "all four -100",
            SlidersPayload {
                highlights: -100.0,
                shadows: -100.0,
                whites: -100.0,
                blacks: -100.0,
                ..defaults()
            },
        ),
        (
            "every tone control at its rail",
            SlidersPayload {
                contrast: 100.0,
                brightness: 100.0,
                highlights: 100.0,
                shadows: 100.0,
                whites: 100.0,
                blacks: 100.0,
                ..defaults()
            },
        ),
        (
            "every tone control at its other rail",
            SlidersPayload {
                contrast: -100.0,
                brightness: -100.0,
                highlights: -100.0,
                shadows: -100.0,
                whites: -100.0,
                blacks: -100.0,
                ..defaults()
            },
        ),
    ];

    for (label, sliders) in cases {
        assert_tone_order(label, 65, &sliders);
    }
}

// ── Saturation ────────────────────────────────────────────────────────────────

#[test]
fn saturation_at_the_bottom_rail_gives_neutral_grey() {
    let sliders = SlidersPayload {
        saturation: -100.0,
        ..defaults()
    };

    for patch in [[200u8, 60, 60], [40, 160, 150], [30, 30, 220]] {
        let got = develop_patch(patch, &sliders);
        assert!(
            got[0] == got[1] && got[1] == got[2],
            "saturation -100 must fully desaturate {patch:?}, got {got:?}"
        );

        // ...and it must be the BT.709 luma of the linear input, not some other
        // grey: a wrong weight set here shows up as tonal drift in B&W mode.
        let luma: f64 = [0.2126, 0.7152, 0.0722]
            .iter()
            .zip(patch.iter())
            .map(|(w, &c)| w * srgb_to_linear(c as f64 / 255.0))
            .sum();
        let want = (linear_to_srgb(luma.clamp(0.0, 1.0)) * 255.0).round() as u8;
        assert!(
            close(got[0], want),
            "saturation -100 on {patch:?}: expected luma ~{want}, got {}",
            got[0]
        );
    }
}

#[test]
fn positive_saturation_widens_the_channel_spread() {
    let patch = [200u8, 120, 90];
    let spread = |p: [u8; 3]| {
        *p.iter().max().unwrap() as i32 - *p.iter().min().unwrap() as i32
    };

    let base = spread(develop_patch(patch, &defaults()));
    let hot = spread(develop_patch(
        patch,
        &SlidersPayload {
            saturation: 60.0,
            ..defaults()
        },
    ));
    let cool = spread(develop_patch(
        patch,
        &SlidersPayload {
            saturation: -60.0,
            ..defaults()
        },
    ));

    assert!(hot > base, "saturation +60 should widen the spread: {base} → {hot}");
    assert!(cool < base, "saturation -60 should narrow the spread: {base} → {cool}");
}

// ── Vibrance ──────────────────────────────────────────────────────────────────

/// Vibrance is saturation weighted by how unsaturated the pixel already is.
/// Testing it against a single patch cannot tell it apart from saturation, so
/// this compares two patches with different starting saturation.
#[test]
fn vibrance_boosts_muted_colours_more_than_vivid_ones() {
    let muted = [140u8, 120, 120];
    let vivid = [220u8, 30, 30];
    let sliders = SlidersPayload {
        vibrance: 100.0,
        ..defaults()
    };

    let spread = |p: [u8; 3]| {
        *p.iter().max().unwrap() as i32 - *p.iter().min().unwrap() as i32
    };

    let muted_gain = spread(develop_patch(muted, &sliders)) - spread(develop_patch(muted, &defaults()));
    let vivid_gain = spread(develop_patch(vivid, &sliders)) - spread(develop_patch(vivid, &defaults()));

    assert!(
        muted_gain > 0,
        "vibrance should saturate a muted colour, gain was {muted_gain}"
    );
    assert!(
        muted_gain > vivid_gain,
        "vibrance should favour muted colours: muted gained {muted_gain}, vivid gained {vivid_gain}"
    );
}

#[test]
fn vibrance_leaves_a_neutral_neutral() {
    for vibrance in [-100.0f32, 100.0] {
        let got = develop_patch(
            [128; 3],
            &SlidersPayload {
                vibrance,
                ..defaults()
            },
        );
        assert!(
            got.iter().all(|&c| close(c, 128)),
            "vibrance {vibrance} tinted a neutral: {got:?}"
        );
    }
}

/// Vibrance must not manufacture light where the chain produced none.
///
/// It measures how saturated a pixel already is by dividing by the pixel's
/// brightest channel. A negative `blacks` or `shadows` pushes that channel
/// below zero, and the unguarded divisor then crossed zero and took the boost
/// with it, hauling pixels the tone curve had legitimately crushed to black
/// back into view. On a dark ramp that renders as 0,0,0,0,1,3 at vibrance 0,
/// the old chain returned 2,6,24,0,0 — bright speckle scattered through the
/// shadows, at its worst a tenth of the way up the range.
///
/// The invariant is deliberately one-sided. Vibrance is a per-pixel gain, so
/// unlike the tone controls it cannot promise strict tone order — the factor
/// legitimately differs between neighbouring pixels of different saturation,
/// which moves 8-bit output by a code value here and there. What it can promise
/// is that it never lifts black off the floor.
#[test]
fn vibrance_does_not_resurrect_crushed_shadows() {
    // A warm ramp. Neutral pixels have no saturation to measure, so they never
    // reach the ratio that used to blow up.
    let img: Rgba16Image = ImageBuffer::from_fn(256, 1, |x, _| {
        let v = x as u8;
        Rgba([
            u8_to_u16(v),
            u8_to_u16(v.saturating_sub(6)),
            u8_to_u16(v.saturating_sub(10)),
            u16::MAX,
        ])
    });

    for (label, crushed) in [
        ("blacks at the bottom rail", SlidersPayload { blacks: -100.0, ..defaults() }),
        ("shadows at the bottom rail", SlidersPayload { shadows: -100.0, ..defaults() }),
        (
            "blacks and shadows together",
            SlidersPayload { blacks: -100.0, shadows: -100.0, ..defaults() },
        ),
    ] {
        let reference = develop(&img, &crushed);
        let boosted = develop(&img, &SlidersPayload { vibrance: 100.0, ..crushed });

        let mut lifted = 0;
        for (i, (&r, &b)) in reference.iter().zip(boosted.iter()).enumerate() {
            if r == 0 {
                assert!(
                    b == 0,
                    "{label}: vibrance lit a crushed pixel — byte {i} (input {}, channel {}) \
                     renders 0 without vibrance and {b} with it",
                    i / 3,
                    i % 3
                );
                lifted += 1;
            }
        }
        assert!(
            lifted > 30,
            "{label}: only {lifted} bytes were crushed to black, so this proves little"
        );

        // Guard against the assertion above passing because vibrance did
        // nothing at all: it must still be visibly at work further up the ramp.
        assert!(
            reference != boosted,
            "{label}: vibrance changed nothing anywhere on the ramp"
        );
    }
}

// ── Hue ───────────────────────────────────────────────────────────────────────

/// The rotation is about the (1,1,1) axis, so 120° is an exact permutation of
/// the primaries. That makes it the one hue angle with a closed-form answer.
#[test]
fn a_hundred_and_twenty_degrees_permutes_the_primaries() {
    let red = [220u8, 40, 40];
    let sliders = SlidersPayload {
        hue: 120.0,
        ..defaults()
    };

    let got = develop_patch(red, &sliders);
    let want = [red[1], red[2], red[0]]; // R→B, G→R, B→G

    assert!(
        (0..3).all(|c| close(got[c], want[c])),
        "hue +120 on {red:?}: expected a channel rotation to {want:?}, got {got:?}"
    );
}

#[test]
fn hue_rotation_is_reversible_and_wraps() {
    let patch = [200u8, 90, 40];

    let forward = develop_patch(patch, &SlidersPayload { hue: 60.0, ..defaults() });
    let backward = develop_patch(patch, &SlidersPayload { hue: -60.0, ..defaults() });
    assert!(
        differs(forward, backward),
        "+60 and -60 must rotate opposite ways, both gave {forward:?}"
    );

    let plus = develop_patch(patch, &SlidersPayload { hue: 180.0, ..defaults() });
    let minus = develop_patch(patch, &SlidersPayload { hue: -180.0, ..defaults() });
    assert!(
        (0..3).all(|c| close(plus[c], minus[c])),
        "±180 are the same rotation: {plus:?} vs {minus:?}"
    );
}

#[test]
fn hue_leaves_a_neutral_neutral() {
    for hue in [-180.0f32, -90.0, 45.0, 120.0, 180.0] {
        let got = develop_patch(
            [128; 3],
            &SlidersPayload {
                hue,
                ..defaults()
            },
        );
        assert!(
            got.iter().all(|&c| close(c, 128)),
            "hue {hue} tinted a neutral: {got:?}"
        );
    }
}

// ── Cross-cutting ─────────────────────────────────────────────────────────────

/// The anti-dead-slider guard: every one of the 24 lanes, moved on its own,
/// must change the rendered image. A slider dropped from the packing order, or
/// left unread by the shader, fails here and nowhere else.
#[test]
fn every_slider_changes_the_rendered_image() {
    let chart = mixed_ramp(32);
    let base = develop(&chart, &defaults());

    let cases: Vec<(&str, SlidersPayload)> = vec![
        ("invert", SlidersPayload { invert: true, ..defaults() }),
        ("red_black_point", SlidersPayload { red_black_point: 40.0, ..defaults() }),
        ("green_black_point", SlidersPayload { green_black_point: 40.0, ..defaults() }),
        ("blue_black_point", SlidersPayload { blue_black_point: 40.0, ..defaults() }),
        ("red_white_point", SlidersPayload { red_white_point: 200.0, ..defaults() }),
        ("green_white_point", SlidersPayload { green_white_point: 200.0, ..defaults() }),
        ("blue_white_point", SlidersPayload { blue_white_point: 200.0, ..defaults() }),
        ("rgb_output_black", SlidersPayload { rgb_output_black: 40.0, ..defaults() }),
        ("rgb_output_white", SlidersPayload { rgb_output_white: 200.0, ..defaults() }),
        ("red_gamma", SlidersPayload { red_gamma: 1.8, ..defaults() }),
        ("green_gamma", SlidersPayload { green_gamma: 1.8, ..defaults() }),
        ("blue_gamma", SlidersPayload { blue_gamma: 1.8, ..defaults() }),
        ("wb_temp", SlidersPayload { wb_temp: 3200.0, ..defaults() }),
        ("wb_tint", SlidersPayload { wb_tint: 40.0, ..defaults() }),
        ("exposure", SlidersPayload { exposure: 1.0, ..defaults() }),
        ("contrast", SlidersPayload { contrast: 40.0, ..defaults() }),
        ("brightness", SlidersPayload { brightness: 20.0, ..defaults() }),
        ("highlights", SlidersPayload { highlights: 60.0, ..defaults() }),
        ("shadows", SlidersPayload { shadows: 60.0, ..defaults() }),
        ("whites", SlidersPayload { whites: 60.0, ..defaults() }),
        ("blacks", SlidersPayload { blacks: 60.0, ..defaults() }),
        ("saturation", SlidersPayload { saturation: 50.0, ..defaults() }),
        ("vibrance", SlidersPayload { vibrance: 60.0, ..defaults() }),
        ("hue", SlidersPayload { hue: 60.0, ..defaults() }),
    ];

    assert_eq!(cases.len(), 24, "one case per lane of the Sliders struct");

    let mut inert = Vec::new();
    for (name, sliders) in cases {
        let out = develop(&chart, &sliders);
        let worst = base
            .iter()
            .zip(out.iter())
            .map(|(&a, &b)| (a as i32 - b as i32).abs())
            .max()
            .unwrap_or(0);
        if worst <= TOL {
            inert.push(format!("{name} (worst delta {worst})"));
        }
    }

    assert!(
        inert.is_empty(),
        "these sliders had no effect on the image:\n  {}",
        inert.join("\n  ")
    );
}

/// No slider outside the per-channel and white-balance groups may introduce a
/// colour cast. A transposed matrix or a channel typo shows up here as a tint on
/// an image the user only meant to brighten.
#[test]
fn the_neutral_axis_stays_neutral() {
    let cases: Vec<(&str, SlidersPayload)> = vec![
        ("exposure", SlidersPayload { exposure: 1.5, ..defaults() }),
        ("contrast", SlidersPayload { contrast: 70.0, ..defaults() }),
        ("brightness", SlidersPayload { brightness: 40.0, ..defaults() }),
        ("highlights", SlidersPayload { highlights: 100.0, ..defaults() }),
        ("shadows", SlidersPayload { shadows: 100.0, ..defaults() }),
        ("whites", SlidersPayload { whites: 100.0, ..defaults() }),
        ("blacks", SlidersPayload { blacks: 100.0, ..defaults() }),
        ("saturation", SlidersPayload { saturation: 80.0, ..defaults() }),
        ("vibrance", SlidersPayload { vibrance: 80.0, ..defaults() }),
        ("hue", SlidersPayload { hue: 100.0, ..defaults() }),
        ("output range", SlidersPayload { rgb_output_black: 30.0, rgb_output_white: 220.0, ..defaults() }),
        ("invert", SlidersPayload { invert: true, ..defaults() }),
    ];

    for (label, sliders) in cases {
        let out = develop(&gray_ramp(32, 1), &sliders);
        for x in 0..32 {
            let px = [out[x * 3], out[x * 3 + 1], out[x * 3 + 2]];
            let spread = *px.iter().max().unwrap() as i32 - *px.iter().min().unwrap() as i32;
            assert!(
                spread <= TOL,
                "{label} tinted the neutral ramp at column {x}: {px:?}"
            );
        }
    }
}
