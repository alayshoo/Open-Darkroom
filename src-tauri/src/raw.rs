// src-tauri/src/raw.rs
//
// Camera sensor data, decoded into the same interleaved u16 buffer every other
// source produces.
//
// A RAW file answers the question `SourceSpace` asks differently from a tagged
// JPEG. There is no transfer curve to undo — sensor data is already linear
// light — and the primaries come from the camera's own calibration rather than
// an ICC profile. What leaves this module is therefore ordinary u16 pixels and
// a matrix, and nothing downstream needs to know where they came from.

use rawler::decoders::supported_extensions;
use rawler::imgop::chromatic_adaption::adapt_bradford;
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::imgop::matrix::transform_1d;
use rawler::imgop::sensor::SensorType;
use rawler::imgop::xyz::Illuminant;
use rawler::rawimage::RawPhotometricInterpretation;
use rawler::{Orientation, RawImage};
use rayon::prelude::*;
use std::path::Path;

use crate::color::LUT_SIZE;
use crate::icc::{Matrix3, SourceSpace, D65, SRGB_TO_WORKING, WORKING_PRIMARIES};
use crate::output_space::{column_major, invert, multiply, rgb_to_xyz};

/// Channels a decoded sensor image is handed on as. Sensors carry no alpha, so
/// this is always three and the buffer is a quarter smaller than an RGBA one.
pub const RAW_CHANNELS: usize = 3;

/// The pipeline run over the sensor data, and as much as this module wants from
/// rawler.
///
/// `Calibrate` and `SRgb` are deliberately absent. Calibrate bakes the white
/// balance *and* the colour matrix into the pixels, and converts to sRGB
/// primaries while it is there; the matrix is this pipeline's to apply on the
/// GPU, after the darkroom block, and the destination is Rec.2020. SRgb would
/// gamma-encode data the rest of the pipeline wants linear.
const STEPS: [ProcessingStep; 5] = [
    ProcessingStep::Rescale,
    ProcessingStep::Demosaic,
    ProcessingStep::FujiRotate,
    ProcessingStep::CropActiveArea,
    ProcessingStep::CropDefault,
];

/// Illuminants to take a colour matrix from, best first. Anything but D65 is
/// Bradford-adapted to D65, which is the working space's white.
const ILLUMINANTS: [Illuminant; 9] = [
    Illuminant::D65,
    Illuminant::A,
    Illuminant::B,
    Illuminant::C,
    Illuminant::D50,
    Illuminant::D55,
    Illuminant::D75,
    Illuminant::Daylight,
    Illuminant::Flash,
];

/// A decoded sensor image, in the shape `prepare_image` takes.
pub struct DecodedRaw {
    /// Interleaved u16, three channels, linear light in the camera's own
    /// primaries with the as-shot white balance already applied.
    pub pixels_u16: Vec<u16>,
    pub width: u32,
    pub height: u32,
    pub space: SourceSpace,
}

/// Whether a path names a format this module decodes, by extension.
pub fn is_raw_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_uppercase())
        .is_some_and(|e| supported_extensions().contains(&e.as_str()))
}

/// Every extension this module decodes, lower-cased for the file dialog.
pub fn dialog_extensions() -> Vec<String> {
    supported_extensions()
        .iter()
        .map(|e| e.to_ascii_lowercase())
        .collect()
}

/// Decode a RAW file into linear camera-primaries u16 pixels.
pub fn decode(path: &Path) -> Result<DecodedRaw, String> {
    let raw = rawler::decode_file(path).map_err(|e| format!("Failed to decode RAW file: {e}"))?;

    reject_unsupported_sensor(&raw)?;

    let developed = RawDevelop::new_with(&STEPS)
        .develop_intermediate(&raw)
        .map_err(|e| format!("Failed to develop RAW file: {e}"))?;

    let (width, height) = (developed.dim().w, developed.dim().h);
    let (pixels, space) = match developed {
        // TODO: white balance is baked as-shot here. Temperature and tint
        // belong on a live slider, which needs this buffer left unbalanced and
        // the multipliers moved into develop.wgsl, ahead of the matrix.
        Intermediate::ThreeColor(pixels) => {
            let wb = white_balance(raw.wb_coeffs);
            let out = to_u16(&pixels.data, |px, c| px[c] * wb[c]);
            let (illuminant, matrix) = raw
                .color_matrix_find_first(ILLUMINANTS)
                .ok_or("RAW file carries no colour matrix")?;
            let to_working = camera_to_working(illuminant, &matrix)?;
            (out, SourceSpace::linear_with_matrix(to_working))
        }
        // One channel and no primaries, so it takes the untagged matrix for
        // the reason greyscale profiles do in icc.rs: the darkroom block runs
        // first and is free to tone the three channels apart.
        Intermediate::Monochrome(pixels) => {
            let out = to_u16(&pixels.data, |v, _| *v);
            (out, SourceSpace::linear_with_matrix(SRGB_TO_WORKING))
        }
        Intermediate::FourColor(_) => {
            return Err("Four-colour sensors (RGBE) are not supported yet".into())
        }
    };

    let (pixels, width, height) = orient(pixels, width as u32, height as u32, raw.orientation);

    Ok(DecodedRaw {
        pixels_u16: pixels,
        width,
        height,
        space,
    })
}

/// Sensor layouts this module turns away, before anything expensive runs.
fn reject_unsupported_sensor(raw: &RawImage) -> Result<(), String> {
    // TODO: X-Trans should be supported. rawler only demosaics it bilinearly,
    // which on a 6×6 pattern gives the maze artifacts the layout is known for;
    // it wants a Markesteijn pass first.
    if let RawPhotometricInterpretation::Cfa(config) = &raw.photometric {
        if config.sensor == SensorType::Xtrans {
            return Err("Fujifilm X-Trans sensors are not supported yet".into());
        }
    }
    Ok(())
}

/// As-shot white balance multipliers for R, G and B, scaled so the largest is
/// exactly one.
///
/// Scaling by the largest rather than by green is what keeps the result inside
/// the unit interval, so nothing clips on the way into u16. The image lands
/// darker by roughly the largest coefficient, which the black and white points
/// recover. No precision goes with it: a 14-bit source spreads over the u16
/// range four codes apart, so scaling it down by anything above a quarter
/// still leaves every input code its own output code.
pub fn white_balance(wb_coeffs: [f32; 4]) -> [f32; RAW_CHANNELS] {
    let coeffs: [f32; RAW_CHANNELS] = std::array::from_fn(|c| wb_coeffs[c]);

    // Some older files record no coefficients at all.
    if !coeffs.iter().all(|c| c.is_finite() && *c > 0.0) {
        return [1.0; RAW_CHANNELS];
    }

    let max = coeffs.iter().copied().fold(f32::MIN, f32::max);
    coeffs.map(|c| c / max)
}

/// Camera RGB → linear Rec.2020, column-major for WGSL.
///
/// The camera's calibration gives XYZ → camera; the working space's primaries
/// give XYZ → Rec.2020. Composing the first inverted with the second is the
/// whole conversion. Rows are normalised to sum to one afterwards so that a
/// white-balanced neutral stays neutral — without it the matrix carries a cast
/// of its own, on top of the balance already applied to the pixels.
pub fn camera_to_working(illuminant: Illuminant, flat: &[f32]) -> Result<Matrix3, String> {
    let xyz_to_cam = transform_1d::<3, 3>(flat)
        .ok_or("RAW colour matrix does not describe three channels")?;
    let xyz_to_cam = if illuminant == Illuminant::D65 {
        xyz_to_cam
    } else {
        adapt_bradford(&illuminant, &Illuminant::D65, &xyz_to_cam)
    };

    let xyz_to_cam: [[f64; 3]; 3] =
        std::array::from_fn(|row| std::array::from_fn(|col| xyz_to_cam[row][col] as f64));
    let xyz_to_working = invert(rgb_to_xyz(WORKING_PRIMARIES, D65));
    let cam_to_working = multiply(xyz_to_working, invert(xyz_to_cam));

    let mut out = [[0.0f64; 3]; 3];
    for (row, values) in cam_to_working.iter().enumerate() {
        let sum: f64 = values.iter().sum();
        if sum.abs() < f64::EPSILON {
            return Err("RAW colour matrix maps neutral to zero".into());
        }
        for col in 0..3 {
            out[row][col] = values[col] / sum;
        }
    }
    Ok(column_major(out))
}

/// Scale one pixel's worth of unit-interval floats into the full u16 range,
/// three channels wide. `channel` reads a channel out of a source pixel, which
/// is where the white balance is applied for colour and where a single
/// monochrome sample is replicated.
fn to_u16<T: Sync>(src: &[T], channel: impl Fn(&T, usize) -> f32 + Sync) -> Vec<u16> {
    let scale = (LUT_SIZE - 1) as f32;
    let mut out = vec![0u16; src.len() * RAW_CHANNELS];

    out.par_chunks_exact_mut(RAW_CHANNELS)
        .zip(src.par_iter())
        .for_each(|(dst, px)| {
            for (c, v) in dst.iter_mut().enumerate() {
                *v = (channel(px, c).clamp(0.0, 1.0) * scale).round() as u16;
            }
        });

    out
}

/// Apply the orientation the file records, so everything downstream sees the
/// image the way it was shot.
///
/// Rotating here rather than carrying a flag keeps the dimensions in the buffer
/// honest: the preview, the overview, the histograms and the export all read
/// them, and any one of them left unrotated would disagree with the rest.
pub fn orient(
    src: Vec<u16>,
    width: u32,
    height: u32,
    orientation: Orientation,
) -> (Vec<u16>, u32, u32) {
    let (w, h) = (width as usize, height as usize);

    // `src` is already upright, and most frames are: the copy is worth skipping.
    if matches!(orientation, Orientation::Normal | Orientation::Unknown) {
        return (src, width, height);
    }

    let transposed = matches!(
        orientation,
        Orientation::Transpose
            | Orientation::Rotate90
            | Orientation::Transverse
            | Orientation::Rotate270
    );

    let (dw, dh) = if transposed { (h, w) } else { (w, h) };
    let mut out = vec![0u16; src.len()];

    out.par_chunks_exact_mut(dw * RAW_CHANNELS)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..dw {
                let (sx, sy) = match orientation {
                    Orientation::HorizontalFlip => (w - 1 - x, y),
                    Orientation::Rotate180 => (w - 1 - x, h - 1 - y),
                    Orientation::VerticalFlip => (x, h - 1 - y),
                    Orientation::Transpose => (y, x),
                    Orientation::Rotate90 => (y, h - 1 - x),
                    Orientation::Transverse => (w - 1 - y, h - 1 - x),
                    Orientation::Rotate270 => (w - 1 - y, x),
                    Orientation::Normal | Orientation::Unknown => (x, y),
                };
                let at = (sy * w + sx) * RAW_CHANNELS;
                row[x * RAW_CHANNELS..(x + 1) * RAW_CHANNELS]
                    .copy_from_slice(&src[at..at + RAW_CHANNELS]);
            }
        });

    (out, dw as u32, dh as u32)
}
