// src-tauri/tests/output_space.rs
//
// Layer 1: the destination side of colour management.
//
// `source_space.rs` covers what a file's own profile means on the way in. This
// is the mirror: what the render is converted into on the way out, and whether
// the profile written into the file actually describes it.
//
// The matrices are derived rather than tabulated, so these tests are mostly
// properties — white stays white, a space is its own inverse, a profile read
// back through the input path lands where the output path started. A derivation
// that is subtly wrong satisfies none of them while still producing a plausible
// picture, which is the failure mode worth guarding.

use lcms2::{Locale, Profile, Tag, TagSignature};

use open_darkroom_lib::icc::{Matrix3, SourceSpace, IDENTITY, SRGB_TO_WORKING};
use open_darkroom_lib::output_space::{
    OutputSpace, Transfer, ADOBE_GAMMA, BT2020_ALPHA, BT2020_BETA,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Column-major 3×3 product, the layout WGSL and both matrices use.
fn multiply(a: Matrix3, b: Matrix3) -> Matrix3 {
    let mut out = IDENTITY;
    for col in 0..3 {
        for row in 0..3 {
            out[col * 3 + row] = (0..3).map(|k| a[k * 3 + row] * b[col * 3 + k]).sum();
        }
    }
    out
}

fn apply(m: Matrix3, v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| (0..3).map(|k| m[k * 3 + row] * v[k]).sum())
}

fn assert_close(got: Matrix3, want: Matrix3, tolerance: f32, what: &str) {
    for i in 0..9 {
        assert!(
            (got[i] - want[i]).abs() <= tolerance,
            "{what}: element {i} is {}, expected {} (off by {:.2e})\n got  {got:?}\n want {want:?}",
            got[i],
            want[i],
            (got[i] - want[i]).abs()
        );
    }
}

// ── The matrices ──────────────────────────────────────────────────────────────

/// Choosing the working space as the destination is the one case that converts
/// nothing, which is the whole argument for offering it as the 16-bit default.
/// An identity that is only nearly identity would tint every archival export.
#[test]
fn the_working_space_is_its_own_destination() {
    assert_close(
        OutputSpace::Rec2020.working_to_destination(),
        IDENTITY,
        1e-6,
        "Rec.2020 → Rec.2020",
    );
}

/// Soft-proofing converts into a space and straight back out, so an inverse
/// that is not quite an inverse would shift the preview's colour every time
/// proofing was switched on — a shift that looks exactly like what proofing is
/// supposed to be showing.
#[test]
fn every_space_round_trips_through_its_own_inverse() {
    for space in OutputSpace::ALL {
        assert_close(
            multiply(space.destination_to_working(), space.working_to_destination()),
            IDENTITY,
            1e-5,
            &format!("{space:?} round trip"),
        );
    }
}

/// The untagged source matrix was derived independently and is already covered
/// by `source_space.rs`, so it is an outside check on this derivation: an
/// untagged sRGB file converted in and straight back out has to come back
/// where it started.
#[test]
fn the_srgb_destination_inverts_the_untagged_source_matrix() {
    assert_close(
        multiply(OutputSpace::Srgb.working_to_destination(), SRGB_TO_WORKING),
        IDENTITY,
        1e-5,
        "linear sRGB → working → linear sRGB",
    );
}

/// Every space here shares the working space's white, which is what lets the
/// gamut compression in the shader aim at one neutral axis for all of them. If
/// a white point drifted, neutrals would come out tinted and the compression
/// would pull colour towards the wrong grey.
#[test]
fn white_stays_white_in_every_destination() {
    for space in OutputSpace::ALL {
        let white = apply(space.working_to_destination(), [1.0, 1.0, 1.0]);
        for (channel, value) in white.iter().enumerate() {
            assert!(
                (value - 1.0).abs() < 1e-5,
                "{space:?}: working white lands at {white:?}, channel {channel} off by {:.2e}",
                (value - 1.0).abs()
            );
        }
    }
}

/// The narrower spaces sit inside Rec.2020, so their primaries land on
/// working-space values that are positive or barely not. Having the two spaces
/// the wrong way round instead gives negatives an order of magnitude larger —
/// Rec.2020's own red is about -0.12 in sRGB — and the round-trip test above
/// cannot see that, being symmetric.
///
/// The tolerance is not slack. Display P3's red really is a hair outside
/// Rec.2020: Rec.2020's red is the more saturated of the two, but the straight
/// line from it to Rec.2020's green cuts just inside P3's red point, leaving it
/// 0.00125 outside that edge in xy.
#[test]
fn a_narrower_gamut_fits_inside_the_working_space() {
    for space in [OutputSpace::Srgb, OutputSpace::DisplayP3, OutputSpace::AdobeRgb] {
        let to_working = space.destination_to_working();
        for (name, primary) in [
            ("red", [1.0, 0.0, 0.0]),
            ("green", [0.0, 1.0, 0.0]),
            ("blue", [0.0, 0.0, 1.0]),
        ] {
            let working = apply(to_working, primary);
            assert!(
                working.iter().all(|&c| c >= -5e-3),
                "{space:?} {name} lands well outside Rec.2020 at {working:?}"
            );
        }
    }
}

/// Pinned on its own because it is the one case where the shader's gamut
/// compression has work to do even showing a P3 export on a P3 display, and
/// because a derivation that quietly clamped it would look like a tidier
/// result rather than a wrong one.
#[test]
fn display_p3_red_sits_marginally_outside_the_working_space() {
    let red = apply(OutputSpace::DisplayP3.destination_to_working(), [1.0, 0.0, 0.0]);
    let lowest = red.iter().copied().fold(f32::INFINITY, f32::min);

    assert!(lowest < 0.0, "P3 red is inside Rec.2020 at {red:?}");
    assert!(lowest > -5e-3, "P3 red is far outside Rec.2020 at {red:?}");
}

// ── The profile ───────────────────────────────────────────────────────────────

/// The point of the whole exercise: the profile written into the file has to
/// describe the same conversion the shader was handed.
///
/// Read back through `SourceSpace`, which is the app's own input path, so this
/// is the round trip a user performs by exporting a file and opening it again.
/// A profile that disagreed with the pixels is the one failure colour management
/// cannot survive, and it is invisible until someone opens the file somewhere
/// else.
#[test]
fn the_profile_describes_the_matrix_the_shader_was_given() {
    for space in OutputSpace::ALL {
        let icc = space.icc().expect("profile");
        let resolved = SourceSpace::resolve(Some(icc));

        assert!(
            !resolved.baked,
            "{space:?}: the profile we write is not one we can read as curves and a matrix"
        );

        // ICC colorants are stored as s15Fixed16 and travel through the D50 PCS
        // and back, so this agrees to about four decimals rather than exactly.
        assert_close(
            resolved.to_working,
            space.destination_to_working(),
            2e-3,
            &format!("{space:?}: profile primaries vs the matrix the GPU applies"),
        );
    }
}

/// An untitled profile shows as a blank entry wherever a file's space is
/// listed, which reads as "untagged" to anyone checking.
#[test]
fn every_profile_names_itself() {
    for space in OutputSpace::ALL {
        let icc = space.icc().expect("profile");
        let profile = Profile::new_icc(&icc).expect("readable");

        let Tag::MLU(description) = profile.read_tag(TagSignature::ProfileDescriptionTag) else {
            panic!("{space:?}: the profile carries no description");
        };
        assert_eq!(
            description.text(Locale::none()).expect("ascii description"),
            space.description(),
            "{space:?}: the profile calls itself something else"
        );
    }
}

/// The transfer curve is stated twice — as parametric ICC coefficients here and
/// as arithmetic in the shader — and the two have to be the same curve. A
/// mismatch is not a crash: the file is encoded with one curve and tagged with
/// another, so it opens looking washed out or crushed.
#[test]
fn the_profile_curve_matches_the_shader_curve() {
    // The decoding direction, which is what an ICC TRC tag stores.
    fn decode(transfer: Transfer, encoded: f64) -> f64 {
        match transfer {
            Transfer::Srgb => {
                if encoded < 0.04045 {
                    encoded / 12.92
                } else {
                    ((encoded + 0.055) / 1.055).powf(2.4)
                }
            }
            Transfer::Adobe => encoded.powf(ADOBE_GAMMA),
            Transfer::Bt2020 => {
                if encoded < 4.5 * BT2020_BETA {
                    encoded / 4.5
                } else {
                    ((encoded + BT2020_ALPHA - 1.0) / BT2020_ALPHA).powf(1.0 / 0.45)
                }
            }
        }
    }

    for space in OutputSpace::ALL {
        let icc = space.icc().expect("profile");
        let profile = Profile::new_icc(&icc).expect("readable");
        let Tag::ToneCurve(curve) = profile.read_tag(TagSignature::RedTRCTag) else {
            panic!("{space:?}: the profile carries no red tone curve");
        };

        for step in 0..=20 {
            let encoded = step as f64 / 20.0;
            let want = decode(space.transfer(), encoded);
            let got = curve.eval(encoded as f32) as f64;
            assert!(
                (got - want).abs() < 2e-3,
                "{space:?} at {encoded:.2}: the profile decodes to {got:.6}, \
                 the shader's curve to {want:.6}"
            );
        }
    }
}

/// Both halves of the curve have to meet, or the profile has a step in it at
/// the toe — which shows up as banding in the deepest shadows and nowhere else.
#[test]
fn the_bt2020_curve_has_no_step_at_its_toe() {
    let break_point = 4.5 * BT2020_BETA;
    let toe = break_point / 4.5;
    let power = ((break_point + BT2020_ALPHA - 1.0) / BT2020_ALPHA).powf(1.0 / 0.45);

    assert!(
        (toe - power).abs() < 1e-9,
        "the BT.2020 segments meet at {toe:.12} and {power:.12}"
    );
    assert!(
        (toe - BT2020_BETA).abs() < 1e-9,
        "the break should land on beta, got {toe:.12}"
    );
}

/// sRGB and Display P3 differ only in their primaries, and Adobe RGB and sRGB
/// share a red and a blue. Both are easy to get wrong by copying a row, and
/// neither mistake produces an obviously broken image.
#[test]
fn the_spaces_are_actually_different_spaces() {
    let matrices: Vec<Matrix3> = OutputSpace::ALL
        .iter()
        .map(|s| s.working_to_destination())
        .collect();

    for (i, a) in matrices.iter().enumerate() {
        for (j, b) in matrices.iter().enumerate().skip(i + 1) {
            let difference: f32 = (0..9).map(|k| (a[k] - b[k]).abs()).sum();
            assert!(
                difference > 1e-3,
                "{:?} and {:?} describe the same conversion",
                OutputSpace::ALL[i],
                OutputSpace::ALL[j]
            );
        }
    }
}
