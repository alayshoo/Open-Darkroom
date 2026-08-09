// src-tauri/tests/sharpness.rs
//
// The sharpness block — clarity, texture and the four unsharp mask controls.
//
// Only the sliders exist so far: the values travel from the UI through the IPC
// payload and into the GPU uniform, but no step of the develop chain reads them.
// That makes "inert" the contract to test, and these do exactly that — the
// values land in their own lanes, disturb nothing that came before them, and
// change neither the computed params nor a rendered pixel.
//
// When the mask itself lands, the two inertness tests below are the ones that
// must be replaced with real numerical probes; everything else still holds.

mod common;

use common::{develop, mixed_ramp, run_calc_params};

use open_darkroom_lib::export_rendering::{
    sliders_to_bytes, SlidersPayload, SLIDERS_BYTES, SLIDER_COUNT,
};

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

// ── Inertness (GPU) ───────────────────────────────────────────────────────────

#[test]
fn sharpness_does_not_change_the_computed_params() {
    let neutral = run_calc_params(&SlidersPayload::default());
    let maxed = run_calc_params(&railed());

    assert_eq!(
        neutral.0, maxed.0,
        "calcParams does not read the sharpness sliders yet, so Params must be identical"
    );
}

#[test]
fn sharpness_does_not_change_a_rendered_image() {
    let chart = mixed_ramp(64);

    let neutral = develop(&chart, &SlidersPayload::default());
    let maxed = develop(&chart, &railed());

    assert_eq!(
        neutral, maxed,
        "the develop chain does not read the sharpness sliders yet, so the render must be identical"
    );
}
