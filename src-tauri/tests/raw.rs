// src-tauri/tests/raw.rs
//
// Layer 1: sensor data — the three decisions this pipeline makes about a RAW
// file that nothing downstream could catch.
//
// The colour matrix is checked against the published XYZ → sRGB matrix rather
// than against our own numbers: feed a camera whose primaries *are* sRGB and
// the conversion has to land on the constant an untagged file already uses.
// Orientation is checked exhaustively because all eight cases produce an image,
// and seven of them produce the wrong one.

use rawler::imgop::xyz::Illuminant;
use rawler::Orientation;

use open_darkroom_lib::icc::SRGB_TO_WORKING;
use open_darkroom_lib::raw::{camera_to_working, is_raw_extension, orient, white_balance};

// ── Colour ────────────────────────────────────────────────────────────────────

/// XYZ (D65) → linear sRGB, from the sRGB specification. A camera carrying this
/// as its calibration is one whose primaries are exactly sRGB's.
const XYZ_TO_SRGB: [f32; 9] = [
    3.2404542, -1.5371385, -0.4985314, //
    -0.9692660, 1.8760108, 0.0415560, //
    0.0556434, -0.2040259, 1.0572252,
];

#[test]
fn srgb_camera_lands_on_the_untagged_matrix() {
    let got = camera_to_working(Illuminant::D65, &XYZ_TO_SRGB).expect("matrix");

    for (i, (&got, &want)) in got.iter().zip(SRGB_TO_WORKING.iter()).enumerate() {
        assert!(
            (got - want).abs() < 2e-4,
            "element {i}: got {got}, want {want}"
        );
    }
}

#[test]
fn neutral_stays_neutral() {
    // Rows sum to one, so camera RGB (1, 1, 1) maps to working (1, 1, 1). The
    // matrix is column-major, so a row is every third element.
    let m = camera_to_working(Illuminant::D65, &XYZ_TO_SRGB).expect("matrix");

    for row in 0..3 {
        let sum: f32 = (0..3).map(|col| m[col * 3 + row]).sum();
        assert!((sum - 1.0).abs() < 1e-5, "row {row} sums to {sum}");
    }
}

/// XYZ (D65) → camera, for a camera whose primaries are nothing like the
/// working space's: the published Adobe calibration for the Canon EOS 5D Mk IV.
const XYZ_TO_5D4: [f32; 9] = [
    0.6446, -0.0366, -0.0864, //
    -0.4436, 1.2204, 0.2513, //
    -0.0952, 0.2496, 0.6348,
];

/// XYZ (D65) → linear Rec.2020, from the Rec.2020 specification.
const XYZ_TO_REC2020: [[f64; 3]; 3] = [
    [1.7166512, -0.3556708, -0.2533663],
    [-0.6666844, 1.6164812, 0.0157685],
    [0.0176399, -0.0427706, 0.9421031],
];

#[test]
fn the_balance_scales_camera_channels_not_output_channels() {
    // The conversion itself is XYZ → Rec.2020 composed with camera → XYZ. What
    // the normalisation is allowed to add to it is one scale per *camera*
    // channel, standing in for the balance already applied to the pixels — so
    // every element of a column is off the raw composition by the same factor.
    // Scaling output channels instead would also leave neutrals neutral, which
    // is why a camera with the working space's own primaries cannot tell the
    // two apart.
    let composed = multiply(XYZ_TO_REC2020, invert(as_rows(&XYZ_TO_5D4)));
    let m = camera_to_working(Illuminant::D65, &XYZ_TO_5D4).expect("matrix");

    for col in 0..3 {
        let scale = m[col * 3] as f64 / composed[0][col];
        for row in 1..3 {
            let got = m[col * 3 + row] as f64 / composed[row][col];
            assert!(
                (got - scale).abs() < 1e-3 * scale.abs(),
                "column {col} row {row}: scaled by {got}, column scaled by {scale}"
            );
        }
    }
}

#[test]
fn a_matrix_of_the_wrong_width_is_refused() {
    assert!(camera_to_working(Illuminant::D65, &[1.0, 0.0, 0.0]).is_err());
}

#[test]
fn a_matrix_that_is_not_numbers_is_refused() {
    let mut flat = XYZ_TO_SRGB;
    flat[4] = f32::NAN;

    assert!(camera_to_working(Illuminant::D65, &flat).is_err());
}

#[test]
fn a_matrix_that_cannot_be_inverted_is_refused() {
    // Two identical rows: the camera cannot tell two of its channels apart, so
    // there is no way back. Nothing may reach the GPU but an error.
    let flat = [
        1.0, 0.0, 0.0, //
        1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0,
    ];

    assert!(camera_to_working(Illuminant::D65, &flat).is_err());
    assert!(camera_to_working(Illuminant::D65, &[0.0f32; 9]).is_err());
}

fn as_rows(flat: &[f32; 9]) -> [[f64; 3]; 3] {
    std::array::from_fn(|row| std::array::from_fn(|col| flat[row * 3 + col] as f64))
}

fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|row| {
        std::array::from_fn(|col| (0..3).map(|k| a[row][k] * b[k][col]).sum())
    })
}

fn invert(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let cofactor = |row: usize, col: usize| {
        let r = [(row + 1) % 3, (row + 2) % 3];
        let c = [(col + 1) % 3, (col + 2) % 3];
        m[r[0]][c[0]] * m[r[1]][c[1]] - m[r[0]][c[1]] * m[r[1]][c[0]]
    };
    let det = (0..3).map(|col| m[0][col] * cofactor(0, col)).sum::<f64>();

    let mut out = [[0.0f64; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            out[col][row] = cofactor(row, col) / det;
        }
    }
    out
}

// ── White balance ─────────────────────────────────────────────────────────────

#[test]
fn white_balance_is_scaled_by_its_largest_coefficient() {
    let wb = white_balance([2.0, 1.0, 1.5, f32::NAN]);

    assert_eq!(wb, [1.0, 0.5, 0.75]);
    assert!(
        wb.iter().all(|&c| c <= 1.0),
        "no coefficient may push a sample past unity"
    );
}

#[test]
fn white_balance_preserves_a_neutral() {
    // Scaling all three by a constant keeps equal channels equal, which is what
    // lets the matrix be row-normalised independently of the balance.
    let wb = white_balance([2.0, 1.0, 1.5, 1.0]);
    let neutral = [0.5 / 2.0, 0.5 / 1.0, 0.5 / 1.5];
    let balanced: Vec<f32> = neutral.iter().zip(wb).map(|(v, c)| v * c).collect();

    assert!(
        balanced.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-6),
        "{balanced:?} is not neutral"
    );
}

#[test]
fn missing_coefficients_fall_back_to_unity() {
    for coeffs in [
        [f32::NAN, 1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0, 1.0],
        [f32::INFINITY, 1.0, 1.0, 1.0],
    ] {
        assert_eq!(white_balance(coeffs), [1.0, 1.0, 1.0], "{coeffs:?}");
    }
}

// ── Orientation ───────────────────────────────────────────────────────────────

/// A 3 × 2 image whose every pixel names its own position, so a transform that
/// transposes when it should flip cannot pass by symmetry. The red channel is
/// the column, the green channel the row.
fn marked() -> Vec<u16> {
    let mut px = Vec::new();
    for y in 0..2u16 {
        for x in 0..3u16 {
            px.extend_from_slice(&[x, y, 0]);
        }
    }
    px
}

/// Read back the (column, row) a pixel was marked with.
fn at(px: &[u16], w: u32, x: u32, y: u32) -> (u16, u16) {
    let i = ((y * w + x) * 3) as usize;
    (px[i], px[i + 1])
}

#[test]
fn normal_is_not_copied_or_changed() {
    let (out, w, h) = orient(marked(), 3, 2, Orientation::Normal);

    assert_eq!((w, h), (3, 2));
    assert_eq!(out, marked());
}

#[test]
fn flips_keep_the_dimensions() {
    for (orientation, corner) in [
        (Orientation::HorizontalFlip, (2, 0)),
        (Orientation::VerticalFlip, (0, 1)),
        (Orientation::Rotate180, (2, 1)),
    ] {
        let (out, w, h) = orient(marked(), 3, 2, orientation);

        assert_eq!((w, h), (3, 2), "{orientation:?}");
        assert_eq!(at(&out, w, 0, 0), corner, "{orientation:?}");
    }
}

#[test]
fn quarter_turns_swap_the_dimensions() {
    for (orientation, corner) in [
        (Orientation::Transpose, (0, 0)),
        (Orientation::Rotate90, (0, 1)),
        (Orientation::Transverse, (2, 1)),
        (Orientation::Rotate270, (2, 0)),
    ] {
        let (out, w, h) = orient(marked(), 3, 2, orientation);

        assert_eq!((w, h), (2, 3), "{orientation:?}");
        assert_eq!(at(&out, w, 0, 0), corner, "{orientation:?}");
    }
}

#[test]
fn every_orientation_keeps_every_pixel() {
    for orientation in [
        Orientation::Normal,
        Orientation::HorizontalFlip,
        Orientation::Rotate180,
        Orientation::VerticalFlip,
        Orientation::Transpose,
        Orientation::Rotate90,
        Orientation::Transverse,
        Orientation::Rotate270,
        Orientation::Unknown,
    ] {
        let (out, w, h) = orient(marked(), 3, 2, orientation);

        assert_eq!(out.len(), marked().len(), "{orientation:?}");
        assert_eq!((w * h) as usize * 3, out.len(), "{orientation:?}");

        let mut seen: Vec<(u16, u16)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| at(&out, w, x, y))
            .collect();
        seen.sort_unstable();
        let mut want: Vec<(u16, u16)> = (0..2u16)
            .flat_map(|y| (0..3u16).map(move |x| (x, y)))
            .collect();
        want.sort_unstable();

        assert_eq!(seen, want, "{orientation:?}");
    }
}

#[test]
fn rotating_four_quarter_turns_returns_the_original() {
    let (mut px, mut w, mut h) = (marked(), 3u32, 2u32);
    for _ in 0..4 {
        (px, w, h) = orient(px, w, h, Orientation::Rotate90);
    }

    assert_eq!((w, h), (3, 2));
    assert_eq!(px, marked());
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

#[test]
fn raw_extensions_are_recognised_whatever_their_case() {
    for name in ["a.cr3", "a.CR3", "a.NEF", "a.dng", "a.arw", "a.Raf"] {
        assert!(is_raw_extension(std::path::Path::new(name)), "{name}");
    }
    for name in ["a.jpg", "a.JPEG", "a.png", "a.tiff", "a"] {
        assert!(!is_raw_extension(std::path::Path::new(name)), "{name}");
    }
}
