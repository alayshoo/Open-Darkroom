// src-tauri/src/output_space.rs
//
// Where an exported file's numbers land, and what the file says they mean.
//
// The input side resolves what a file's own profile meant (icc.rs); this is the
// mirror of it. A destination is three things that have to agree: the matrix the
// shader applies, the transfer curve it encodes with, and the ICC profile
// written into the file. All three are derived here from one set of published
// chromaticities, so a space cannot be described one way to the GPU and another
// way to whatever opens the file.
//
// Every space here is D65, which is what lets the gamut compression in
// chunks/outputTransform.wgsl treat a working-space luminance as the neutral
// axis of the destination without carrying a second white point around.

use lcms2::{CIExyY, CIExyYTRIPLE, Locale, Profile, Tag, TagSignature, ToneCurve, MLU};

use crate::icc::{Matrix3, D65, WORKING_PRIMARIES};

/// The transfer curve a destination encodes with.
///
/// The discriminants are the contract with `TRANSFER_*` in
/// chunks/outputTransform.wgsl and `TRANSFER` in src/lib/gpu/outputSpace.ts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transfer {
    /// The sRGB piecewise curve, used by sRGB and Display P3 alike.
    Srgb = 0,
    /// Adobe RGB (1998)'s pure power function.
    Adobe = 1,
    /// The BT.2020 OETF, with its own linear toe.
    Bt2020 = 2,
}

/// Adobe RGB (1998) specifies 2 + 51/256; the file format stores it as 563/256.
pub const ADOBE_GAMMA: f64 = 563.0 / 256.0;

/// BT.2020's OETF constants, at the precision the recommendation states them.
pub const BT2020_ALPHA: f64 = 1.099_296_826_809_442;
pub const BT2020_BETA: f64 = 0.018_053_968_510_807;

/// A colour space an export can be written in.
#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub enum OutputSpace {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
    Rec2020,
}

impl OutputSpace {
    /// Every space, for the tests and for anything that has to cover the set.
    pub const ALL: [Self; 4] = [
        Self::Srgb,
        Self::DisplayP3,
        Self::AdobeRgb,
        Self::Rec2020,
    ];

    /// The published chromaticities of the red, green and blue primaries.
    ///
    /// Must stay equal to `PRIMARIES` in src/lib/gpu/outputSpace.ts, which is
    /// what `shader_contracts.rs` checks. These are the only numbers duplicated
    /// across the two languages; both derive their matrices from them.
    pub fn primaries(self) -> [[f64; 2]; 3] {
        match self {
            Self::Srgb => [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060]],
            Self::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
            Self::AdobeRgb => [[0.640, 0.330], [0.210, 0.710], [0.150, 0.060]],
            Self::Rec2020 => WORKING_PRIMARIES,
        }
    }

    pub fn transfer(self) -> Transfer {
        match self {
            // Display P3 is DCI-P3's primaries with sRGB's white and curve.
            Self::Srgb | Self::DisplayP3 => Transfer::Srgb,
            Self::AdobeRgb => Transfer::Adobe,
            Self::Rec2020 => Transfer::Bt2020,
        }
    }

    /// What the profile calls itself, which is what a photo editor shows when
    /// asked which space a file is in.
    pub fn description(self) -> &'static str {
        match self {
            Self::Srgb => "sRGB IEC61966-2.1",
            Self::DisplayP3 => "Display P3",
            Self::AdobeRgb => "Adobe RGB (1998) compatible",
            Self::Rec2020 => "ITU-R BT.2020",
        }
    }

    /// Linear working space → linear destination, column-major for WGSL.
    ///
    /// Both spaces are D65, so this is the destination's primaries undone and
    /// the working space's applied, with no chromatic adaptation between them.
    pub fn working_to_destination(self) -> Matrix3 {
        let working = rgb_to_xyz(WORKING_PRIMARIES, D65);
        let destination = rgb_to_xyz(self.primaries(), D65);
        column_major(multiply(invert(destination), working))
    }

    /// Linear destination → linear working space, the inverse of the above.
    /// Soft-proofing needs the round trip: into the space being proofed, and
    /// back out to carry on to the display.
    pub fn destination_to_working(self) -> Matrix3 {
        let working = rgb_to_xyz(WORKING_PRIMARIES, D65);
        let destination = rgb_to_xyz(self.primaries(), D65);
        column_major(multiply(invert(working), destination))
    }

    /// The ICC profile describing this space, for embedding in an exported file.
    ///
    /// Built rather than shipped: the primaries and the curve are already stated
    /// above for the GPU's benefit, and generating the profile from the same
    /// numbers is what keeps the tag honest about the pixels.
    ///
    /// `greyscale` describes a file written with one sample per pixel. A profile
    /// has to name as many channels as the file carries, so that one is a grey
    /// profile: the same transfer curve, and no primaries, since a single
    /// channel has none. The pixels it describes are neutral, which is what
    /// makes dropping the primaries lossless rather than an approximation.
    pub fn icc(self, greyscale: bool) -> Result<Vec<u8>, String> {
        let curve = self.tone_curve()?;
        let primaries = self.primaries();

        let mut profile = if greyscale {
            Profile::new_gray(&xy_y(D65), &curve)
        } else {
            Profile::new_rgb(
                &xy_y(D65),
                &CIExyYTRIPLE {
                    Red: xy_y(primaries[0]),
                    Green: xy_y(primaries[1]),
                    Blue: xy_y(primaries[2]),
                },
                &[&curve, &curve, &curve],
            )
        }
        .map_err(|e| format!("Failed to build the {} profile: {e}", self.description()))?;

        // An untitled profile shows as a blank entry in anything that lists the
        // space a file is in, so the description is part of being tagged at all.
        let mut description = MLU::new(1);
        description.set_text_ascii(self.description(), Locale::none());
        if !profile.write_tag(TagSignature::ProfileDescriptionTag, Tag::MLU(&description)) {
            return Err(format!(
                "Failed to name the {} profile",
                self.description()
            ));
        }

        profile
            .icc()
            .map_err(|e| format!("Failed to serialise the {} profile: {e}", self.description()))
    }

    /// The profile's transfer curve, in the device → linear direction an ICC
    /// TRC tag is defined in.
    fn tone_curve(self) -> Result<ToneCurve, String> {
        let curve = match self.transfer() {
            // Parametric type 4: (aX + b)^g above the break, cX below it.
            Transfer::Srgb => ToneCurve::new_parametric(
                4,
                &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045],
            ),
            Transfer::Adobe => Ok(ToneCurve::new(ADOBE_GAMMA)),
            // The same parametric shape, solved for the BT.2020 OETF: the toe
            // has slope 4.5 and meets the power segment at beta.
            Transfer::Bt2020 => ToneCurve::new_parametric(
                4,
                &[
                    1.0 / 0.45,
                    1.0 / BT2020_ALPHA,
                    (BT2020_ALPHA - 1.0) / BT2020_ALPHA,
                    1.0 / 4.5,
                    4.5 * BT2020_BETA,
                ],
            ),
        };
        curve.map_err(|e| format!("Failed to build the {} tone curve: {e}", self.description()))
    }
}

fn xy_y(xy: [f64; 2]) -> CIExyY {
    CIExyY {
        x: xy[0],
        y: xy[1],
        Y: 1.0,
    }
}

/// Linear RGB → XYZ for a set of primaries and a white point, by rows.
///
/// Each primary contributes a column scaled so that RGB (1, 1, 1) lands exactly
/// on the white point — the standard construction, and the reason the matrix
/// cannot simply be read off the chromaticities.
pub(crate) fn rgb_to_xyz(primaries: [[f64; 2]; 3], white: [f64; 2]) -> [[f64; 3]; 3] {
    let mut m = [[0.0f64; 3]; 3];
    for (col, xy) in primaries.iter().enumerate() {
        let (x, y) = (xy[0], xy[1]);
        m[0][col] = x / y;
        m[1][col] = 1.0;
        m[2][col] = (1.0 - x - y) / y;
    }

    let w = [white[0] / white[1], 1.0, (1.0 - white[0] - white[1]) / white[1]];
    let scale = multiply_vec(invert(m), w);

    for col in 0..3 {
        for row in 0..3 {
            m[row][col] *= scale[col];
        }
    }
    m
}

pub(crate) fn multiply(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0f64; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            out[row][col] = (0..3).map(|k| a[row][k] * b[k][col]).sum();
        }
    }
    out
}

/// Flatten a row-major 3×3 into the column-major layout WGSL indexes a
/// `mat3x3` by.
pub(crate) fn column_major(m: [[f64; 3]; 3]) -> Matrix3 {
    let mut flat = [0.0f32; 9];
    for col in 0..3 {
        for row in 0..3 {
            flat[col * 3 + row] = m[row][col] as f32;
        }
    }
    flat
}

fn multiply_vec(a: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| (0..3).map(|k| a[row][k] * v[k]).sum())
}

/// Inverse by cofactors. Every matrix reaching this is built from three
/// independent primaries, so the determinant cannot vanish.
pub(crate) fn invert(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let cofactor = |row: usize, col: usize| {
        let r = [(row + 1) % 3, (row + 2) % 3];
        let c = [(col + 1) % 3, (col + 2) % 3];
        m[r[0]][c[0]] * m[r[1]][c[1]] - m[r[0]][c[1]] * m[r[1]][c[0]]
    };

    let det = (0..3).map(|col| m[0][col] * cofactor(0, col)).sum::<f64>();

    // Transposed on the way out: the cofactor of (row, col) belongs at
    // (col, row) in the inverse.
    let mut out = [[0.0f64; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            out[col][row] = cofactor(row, col) / det;
        }
    }
    out
}
