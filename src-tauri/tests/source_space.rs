// src-tauri/tests/source_space.rs
//
// Layer 1: the source colour space — what a file's profile is read as, and how
// that reaches the GPU.
//
// Nothing downstream can tell a mistake here from a deliberate look. A
// transposed matrix, a channel routed through the wrong curve, or a column
// written four bytes early all produce a plausible image in the wrong colours,
// and every slider the user then moves is compensating for it. So the matrices
// are checked against profiles lcms2 builds itself rather than against our own
// restatement of the same numbers.

use image::{ImageBuffer, Rgba};
use lcms2::{CIExyY, CIExyYTRIPLE, Profile, ToneCurve};

use open_darkroom_lib::color::{linearize_u16_f32, LUT_SIZE};
use open_darkroom_lib::export_rendering::{source_space_to_bytes, SOURCE_SPACE_BYTES};
use open_darkroom_lib::icc::{SourceSpace, SRGB_TO_WORKING};
use open_darkroom_lib::image_opening::{build_payload, prepare_image, PAYLOAD_HEADER_BYTES};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn xyy(x: f64, y: f64) -> CIExyY {
    CIExyY { x, y, Y: 1.0 }
}

const D65: (f64, f64) = (0.3127, 0.3290);

/// An RGB profile with the given primaries and one gamma per channel, as ICC
/// bytes — the shape `SourceSpace::resolve` is expected to decompose.
fn rgb_profile(primaries: [[f64; 2]; 3], gammas: [f64; 3]) -> Vec<u8> {
    let curves: Vec<ToneCurve> = gammas.iter().map(|&g| ToneCurve::new(g)).collect();
    Profile::new_rgb(
        &xyy(D65.0, D65.1),
        &CIExyYTRIPLE {
            Red: xyy(primaries[0][0], primaries[0][1]),
            Green: xyy(primaries[1][0], primaries[1][1]),
            Blue: xyy(primaries[2][0], primaries[2][1]),
        },
        &[&curves[0], &curves[1], &curves[2]],
    )
    .expect("profile")
    .icc()
    .expect("serialise")
}

const SRGB_PRIMARIES: [[f64; 2]; 3] = [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]];
const REC2020_PRIMARIES: [[f64; 2]; 3] = [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]];

fn assert_matrix_close(label: &str, got: [f32; 9], want: [f32; 9], tol: f32) {
    for i in 0..9 {
        assert!(
            (got[i] - want[i]).abs() < tol,
            "{label} element {i}: expected {}, got {} (all: {got:?})",
            want[i],
            got[i]
        );
    }
}

// ── What a profile resolves to ────────────────────────────────────────────────

/// A file with no profile is sRGB, which is what the pipeline assumed
/// unconditionally before profiles were read at all. If this drifts, every
/// untagged image in the world changes colour.
#[test]
fn an_untagged_file_is_srgb_and_converts_on_the_gpu() {
    let space = SourceSpace::default();
    assert!(!space.baked, "the matrix path must stay the default");
    assert_matrix_close("default", space.to_working, SRGB_TO_WORKING, 1e-9);
}

/// lcms2's own sRGB profile has to decompose to the same matrix the untagged
/// path uses. The two are built from completely different routes — one from the
/// profile's colorant tags through the D50 adaptation, the other a constant —
/// so agreement pins the whole adaptation chain, not just the constant.
#[test]
fn a_tagged_srgb_profile_agrees_with_the_untagged_constant() {
    let space = SourceSpace::resolve(Some(
        Profile::new_srgb().icc().expect("serialise sRGB"),
    ));

    assert!(!space.baked, "sRGB is a matrix-and-curves profile");

    // Not tighter than 1.5e-3. The profile route goes through the D50 PCS in
    // s15Fixed16 and back out through Bradford, and lcms2 builds its sRGB
    // colorants against the measured D65 tristimulus while the constant is
    // derived from the chromaticity BT.2020 publishes. Those two D65s differ by
    // ~2e-4 in blue — see the note on D65_XYZ in colorimetry.rs. The bound is
    // still twenty times tighter than any transposition or dropped adaptation.
    assert_matrix_close("sRGB profile", space.to_working, SRGB_TO_WORKING, 1.5e-3);

    // The invariant that has to hold exactly whichever D65 is meant: both
    // spaces are normalised to the same white, so white maps to white.
    for row in 0..3 {
        let sum: f32 = (0..3).map(|col| space.to_working[col * 3 + row]).sum();
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "row {row} of the profile matrix sums to {sum}, so white does not map to white"
        );
    }
}

/// A file already in the working space must come out as the identity. This is
/// the test a transposed or misordered matrix cannot survive: the identity is
/// its own transpose only if every off-diagonal really is zero.
#[test]
fn a_rec2020_profile_decomposes_to_the_identity() {
    let space = SourceSpace::resolve(Some(rgb_profile(REC2020_PRIMARIES, [1.0; 3])));

    assert!(!space.baked);
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    assert_matrix_close("Rec.2020 profile", space.to_working, identity, 1e-3);
}

/// Anything that is not curves-and-a-matrix falls back to converting on the
/// CPU, with the identity handed to the GPU. Getting this wrong the other way —
/// claiming a matrix that was never read — would silently apply the identity to
/// a file that needed a real conversion.
#[test]
fn an_unreadable_profile_falls_back_to_converting_on_the_cpu() {
    let space = SourceSpace::resolve(Some(b"not an icc profile at all".to_vec()));

    assert!(space.baked, "an unusable profile must take the CPU path");
    assert_matrix_close(
        "fallback",
        space.to_working,
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        1e-9,
    );
}

/// And when that CPU conversion is the path taken and it then fails, the open
/// has to fail with it. The pixels are still the source's own code values at
/// that point, while `baked` has already told the GPU and the frontend that
/// they are linear working-space ones — so carrying on renders the file's
/// encoding as if it were light, in the wrong primaries, with nothing saying so.
#[test]
fn a_failed_cpu_conversion_fails_the_open() {
    let space = SourceSpace::resolve(Some(b"not an icc profile at all".to_vec()));
    let img: ImageBuffer<Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(4, 3, Rgba([1000u16, 2000, 3000, u16::MAX]));

    let err = prepare_image(img.as_raw(), 4, 3, 4, &space)
        .err()
        .expect("an unusable profile cannot be converted, and must not be ignored");
    assert!(err.contains("colour profile"), "unhelpful error: {err}");
}

// ── Per-channel curves ────────────────────────────────────────────────────────

/// A profile may carry three different transfer curves, and each channel has to
/// go through its own. Reading one and applying it to all three, or swapping two
/// of them, is a single wrong index — and it looks like a colour cast, which is
/// exactly what the user would try to correct with the sliders.
#[test]
fn each_channel_goes_through_its_own_curve() {
    let space = SourceSpace::resolve(Some(rgb_profile(SRGB_PRIMARIES, [1.0, 2.0, 3.0])));
    let luts = space.luts(|v| v);

    // Half scale through gamma 1, 2 and 3.
    let mid = (LUT_SIZE - 1) / 2;
    let expected = [0.5f32, 0.25, 0.125];
    for (c, want) in expected.iter().enumerate() {
        assert!(
            (luts[c][mid] - want).abs() < 2e-3,
            "channel {c} at half scale: expected ~{want}, got {}",
            luts[c][mid]
        );
    }

    // And the conversion routes them the same way round. A grey input must come
    // out unequal, in that order, which a shared curve could not produce.
    let half = ((LUT_SIZE - 1) / 2) as u16;
    let bytes = linearize_u16_f32(&[half, half, half, u16::MAX], 4, &luts);
    let out: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    for (c, want) in expected.iter().enumerate() {
        assert!(
            (out[c] - want).abs() < 2e-3,
            "converted channel {c}: expected ~{want}, got {}",
            out[c]
        );
    }
}

/// Alpha carries no transfer encoding, so it must not go through a curve. With
/// gamma 3 on blue and a half-scale alpha, a curve applied to alpha would be
/// obvious.
#[test]
fn alpha_is_not_sent_through_a_curve() {
    let space = SourceSpace::resolve(Some(rgb_profile(SRGB_PRIMARIES, [1.0, 2.0, 3.0])));
    let half = ((LUT_SIZE - 1) / 2) as u16;

    let bytes = linearize_u16_f32(&[0, 0, 0, half], 4, &space.luts(|v| v));
    let alpha = f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    assert!(
        (alpha - 0.5).abs() < 1e-4,
        "alpha should scale linearly, got {alpha}"
    );
}

// ── Getting it onto the GPU ───────────────────────────────────────────────────

/// A WGSL `mat3x3<f32>` is three 16-byte columns, not nine packed floats. Packed
/// tightly, every column after the first lands early and the image is skewed by
/// a matrix nobody wrote. The padding bytes are the tell, so they are asserted
/// directly.
#[test]
fn the_uniform_pads_each_matrix_column_to_sixteen_bytes() {
    let mut space = SourceSpace::default();
    space.to_working = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];

    let bytes = source_space_to_bytes(&space);
    assert_eq!(bytes.len(), SOURCE_SPACE_BYTES as usize);

    for col in 0..3 {
        for row in 0..3 {
            let at = col * 16 + row * 4;
            let got = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let want = space.to_working[col * 3 + row];
            assert_eq!(got, want, "column {col} row {row} landed at the wrong offset");
        }
        // The fourth lane of each column is padding and must stay zero.
        let pad = col * 16 + 12;
        assert_eq!(
            &bytes[pad..pad + 4],
            &[0u8; 4],
            "column {col} padding lane is not zero"
        );
    }
}

/// The matrix has to survive the trip to the frontend. `openImage.ts` reads it
/// at a fixed offset, so both the offset and the float order are load-bearing.
#[test]
fn the_payload_carries_the_matrix_and_the_baked_flag() {
    let img: ImageBuffer<Rgba<u16>, Vec<u16>> =
        ImageBuffer::from_pixel(4, 3, Rgba([1000u16, 2000, 3000, u16::MAX]));

    let space = SourceSpace::default();
    let prepared = prepare_image(img.as_raw(), 4, 3, 4, &space).expect("prepare");
    let payload = build_payload(&prepared);

    assert!(payload.len() > PAYLOAD_HEADER_BYTES);

    for i in 0..9 {
        let at = 24 + i * 4;
        let got = f32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        assert_eq!(
            got, SRGB_TO_WORKING[i],
            "payload matrix element {i} does not match the resolved space"
        );
    }

    let flags = u32::from_le_bytes(payload[60..64].try_into().unwrap());
    assert_eq!(flags & 1, 0, "an untagged file is not baked");
}
