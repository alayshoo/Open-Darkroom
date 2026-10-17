// src-tauri/src/icc.rs
//
// What a file says its own numbers mean, and how to get from there into the
// working space.
//
// The conversion is split in two on purpose. The transfer curve is undone here,
// on the CPU, because resampling has to happen in linear light. The primaries
// matrix is handed to the GPU instead, to be applied *after* the darkroom block:
// per-channel black, white and gamma only line up with the film's three dye
// layers while the channels are still separate, and converting primaries mixes
// them — a third of Rec.2020's red comes from sRGB's green.

use lcms2::{
    CIExyY, CIExyYTRIPLE, ColorSpaceSignature, Intent, PixelFormat, Profile, Tag, TagSignature,
    ToneCurve, Transform,
};

use crate::color::{srgb_to_linear, LUT_SIZE};

/// Column-major 3×3, the layout a WGSL `mat3x3<f32>` expects.
pub type Matrix3 = [f32; 9];

pub const IDENTITY: Matrix3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Linear sRGB → linear Rec.2020, for a file that carries no profile.
pub const SRGB_TO_WORKING: Matrix3 = [
    0.627_403_9,
    0.069_097_29,
    0.016_391_44,
    0.329_283_04,
    0.919_540_4,
    0.088_013_31,
    0.043_313_07,
    0.011_362_32,
    0.895_595_25,
];

/// XYZ relative to D50 → linear Rec.2020, with the Bradford adaptation to D65
/// folded in. ICC colorant tags are PCS-relative, and the PCS white is D50.
const XYZ_D50_TO_WORKING: [[f32; 3]; 3] = [
    [1.647_278_6, -0.393_596_94, -0.235_981_06],
    [-0.682_614_76, 1.647_609_8, 0.012_815_895],
    [0.029_664_043, -0.062_919_13, 1.253_431_2],
];

/// Rec.2020 primaries and white, for the profile the baked path converts into.
const WORKING_PRIMARIES: [[f64; 2]; 3] = [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]];
const D65: [f64; 2] = [0.3127, 0.3290];

/// How a source image's numbers reach linear working-space values.
#[derive(Clone)]
pub struct SourceSpace {
    /// The embedded profile, kept so the export path can rebuild its tables
    /// rather than inherit the preview's.
    icc: Option<Vec<u8>>,
    /// Source RGB → linear Rec.2020, for the GPU to apply after the darkroom
    /// block. Identity when the conversion had to be baked in on the CPU.
    pub to_working: Matrix3,
    /// Set when the profile could not be expressed as curves and a matrix, so
    /// the CPU did the whole conversion and the darkroom block runs in the
    /// working space rather than the source's own primaries.
    pub baked: bool,
}

impl Default for SourceSpace {
    /// No profile: assume sRGB, which is what every untagged file in practice
    /// means and what the pipeline assumed unconditionally before.
    fn default() -> Self {
        Self {
            icc: None,
            to_working: SRGB_TO_WORKING,
            baked: false,
        }
    }
}

impl SourceSpace {
    /// Resolve what a file's profile means. `None` is sRGB.
    pub fn resolve(icc: Option<Vec<u8>>) -> Self {
        let Some(bytes) = icc else {
            return Self::default();
        };

        match decompose(&bytes) {
            Some(to_working) => Self {
                icc: Some(bytes),
                to_working,
                baked: false,
            },
            // A LUT profile has no separable channels to preserve, so there is
            // nothing to lose by converting it here in full.
            None => Self {
                icc: Some(bytes),
                to_working: IDENTITY,
                baked: true,
            },
        }
    }

    /// Per-channel tables mapping a u16 source code value to linear light in the
    /// source's own primaries, each entry passed through `map`.
    ///
    /// The three channels get their own table because a profile may carry three
    /// different curves. In practice they are equal, but reading one and
    /// applying it to all three would be an assumption with nothing enforcing
    /// it.
    pub fn luts<T>(&self, map: impl Fn(f32) -> T) -> [Vec<T>; 3] {
        let unit = |i: usize| i as f32 / (LUT_SIZE - 1) as f32;

        // The baked path has already linearised, so its table is the identity.
        if self.baked {
            return std::array::from_fn(|_| (0..LUT_SIZE).map(|i| map(unit(i))).collect());
        }

        let curves = self.icc.as_deref().and_then(|bytes| {
            let profile = Profile::new_icc(bytes).ok()?;
            let trc = [
                TagSignature::RedTRCTag,
                TagSignature::GreenTRCTag,
                TagSignature::BlueTRCTag,
            ];
            // Evaluated eagerly: a Profile is not Sync, and 3 × 65536 float
            // evaluations cost far less than the decode around them.
            Some(trc.map(|sig| match profile.read_tag(sig) {
                Tag::ToneCurve(curve) => (0..LUT_SIZE).map(|i| curve.eval(unit(i))).collect(),
                _ => (0..LUT_SIZE).map(|i| srgb_to_linear(unit(i))).collect(),
            }))
        });

        match curves {
            Some(linear) => linear.map(|c: Vec<f32>| c.into_iter().map(&map).collect()),
            None => std::array::from_fn(|_| {
                (0..LUT_SIZE).map(|i| map(srgb_to_linear(unit(i)))).collect()
            }),
        }
    }

    /// Convert interleaved u16 source pixels into interleaved *linear Rec.2020*
    /// u16 pixels, for a profile that could not be split into curves and a
    /// matrix. `Ok(None)` on every other path, where [`Self::luts`] does the
    /// work; `Err` only when this path was the one to take and it failed, which
    /// has to reach the caller — the pixels are untouched source values while
    /// everything downstream has been told they are linear working-space ones.
    ///
    /// The result is u16 rather than float so that it lands in the same
    /// resampling path as every other image. That quantises, and clips whatever
    /// the profile places outside Rec.2020 — acceptable here because a LUT
    /// profile maps into a bounded gamut to begin with, and because the
    /// alternative is a second resampling path for the rarest input.
    pub fn bake(&self, src: &[u16], channels: usize) -> Result<Option<Vec<u16>>, String> {
        if !self.baked {
            return Ok(None);
        }

        let icc = self
            .icc
            .as_deref()
            .ok_or("colour profile missing from a baked source space")?;
        let profile =
            Profile::new_icc(icc).map_err(|e| format!("Failed to read colour profile: {e}"))?;
        let linear = ToneCurve::new(1.0);
        let working = Profile::new_rgb(
            &CIExyY {
                x: D65[0],
                y: D65[1],
                Y: 1.0,
            },
            &CIExyYTRIPLE {
                Red: xy_y(WORKING_PRIMARIES[0]),
                Green: xy_y(WORKING_PRIMARIES[1]),
                Blue: xy_y(WORKING_PRIMARIES[2]),
            },
            &[&linear, &linear, &linear],
        )
        .map_err(|e| format!("Failed to build the working-space profile: {e}"))?;

        // The pixel type has to be three wide, not one: lcms2 checks it against
        // the format's bytes per pixel and panics on a mismatch, so a `u16`
        // element type against RGB_16 never reaches the conversion at all.
        let transform: Transform<[u16; 3], [u16; 3]> = Transform::new(
            &profile,
            PixelFormat::RGB_16,
            &working,
            PixelFormat::RGB_16,
            Intent::RelativeColorimetric,
        )
        .map_err(|e| format!("Failed to build the colour conversion: {e}"))?;

        // Transformed three-wide and rewoven afterwards: lcms2 does not carry
        // an extra channel through unless told to, and alpha has no business in
        // a colour transform anyway.
        let mut rgb: Vec<[u16; 3]> = src
            .chunks_exact(channels)
            .map(|px| [px[0], px[1], px[2]])
            .collect();
        transform.transform_in_place(&mut rgb);

        let mut out = vec![0u16; src.len()];
        for (i, px) in rgb.iter().enumerate() {
            let at = i * channels;
            out[at..at + 3].copy_from_slice(px);
            if channels == 4 {
                out[at + 3] = src[at + 3];
            }
        }
        Ok(Some(out))
    }
}

fn xy_y(xy: [f64; 2]) -> CIExyY {
    CIExyY {
        x: xy[0],
        y: xy[1],
        Y: 1.0,
    }
}

/// Read a profile as three transfer curves plus a primaries matrix, returning
/// source RGB → linear Rec.2020. `None` for anything that is not that shape —
/// a scanner's LUT profile, a CMYK profile, a profile missing a tag.
fn decompose(bytes: &[u8]) -> Option<Matrix3> {
    let profile = Profile::new_icc(bytes).ok()?;

    if profile.color_space() != ColorSpaceSignature::RgbData {
        return None;
    }

    // All six tags have to be present: the colorants give the matrix, the
    // curves the transfer function. A profile carrying only an A2B pipeline has
    // neither, and is the case `bake` exists for.
    let colorants = [
        TagSignature::RedColorantTag,
        TagSignature::GreenColorantTag,
        TagSignature::BlueColorantTag,
    ];
    let curves = [
        TagSignature::RedTRCTag,
        TagSignature::GreenTRCTag,
        TagSignature::BlueTRCTag,
    ];

    if !curves
        .iter()
        .all(|&sig| matches!(profile.read_tag(sig), Tag::ToneCurve(_)))
    {
        return None;
    }

    let mut src_to_xyz = [[0.0f32; 3]; 3];
    for (col, &sig) in colorants.iter().enumerate() {
        let Tag::CIEXYZ(xyz) = profile.read_tag(sig) else {
            return None;
        };
        src_to_xyz[0][col] = xyz.X as f32;
        src_to_xyz[1][col] = xyz.Y as f32;
        src_to_xyz[2][col] = xyz.Z as f32;
    }

    // XYZ_D50_TO_WORKING × src_to_xyz, written out column-major for WGSL.
    let mut out = IDENTITY;
    for col in 0..3 {
        for row in 0..3 {
            out[col * 3 + row] = (0..3)
                .map(|k| XYZ_D50_TO_WORKING[row][k] * src_to_xyz[k][col])
                .sum();
        }
    }
    Some(out)
}
