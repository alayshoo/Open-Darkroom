// src-tauri/examples/make_tagged_jpeg.rs
//
// Generates tests/fixtures/display-p3.jpg, a real JPEG carrying a real ICC
// profile in an APP2 segment. Run with:
//
//     cargo run --example make_tagged_jpeg
//
// Everything else about the source colour space is tested against profiles held
// in memory, which pins the maths but never the step before it: pulling the
// profile back out of a container the decoder parsed. This is the file for
// that, and it is Display P3 rather than sRGB so that ignoring the tag is
// visible in the pixels rather than a no-op.

use std::path::PathBuf;

use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, ImageEncoder};
use lcms2::{CIExyY, CIExyYTRIPLE, Profile, ToneCurve};

/// Display P3: sRGB's white and transfer curve on DCI-P3's primaries.
const P3_PRIMARIES: [[f64; 2]; 3] = [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]];
const D65: [f64; 2] = [0.3127, 0.3290];

/// Flat patches, one per 64-pixel band. Saturated primaries are the colours a
/// wider gamut moves most, and the neutral is the one it must leave alone.
const PATCHES: [[u8; 3]; 4] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [128, 128, 128],
];

const BAND: u32 = 64;

fn xy_y(xy: [f64; 2]) -> CIExyY {
    CIExyY {
        x: xy[0],
        y: xy[1],
        Y: 1.0,
    }
}

fn main() {
    // sRGB's curve is a near-2.2 power function with a linear toe. lcms2 builds
    // it from the parametric type 4 the specification defines it as.
    let srgb_curve = ToneCurve::new_parametric(4, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045])
        .expect("sRGB tone curve");

    let profile = Profile::new_rgb(
        &xy_y(D65),
        &CIExyYTRIPLE {
            Red: xy_y(P3_PRIMARIES[0]),
            Green: xy_y(P3_PRIMARIES[1]),
            Blue: xy_y(P3_PRIMARIES[2]),
        },
        &[&srgb_curve, &srgb_curve, &srgb_curve],
    )
    .expect("Display P3 profile");
    let icc = profile.icc().expect("serialise");

    let (width, height) = (BAND, BAND * PATCHES.len() as u32);
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for patch in PATCHES {
        for _ in 0..width * BAND {
            pixels.extend_from_slice(&patch);
        }
    }

    // Quality 100 and no subsampling: the test reads patch centres back as
    // exact code values, and chroma subsampling would smear the boundaries.
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 100);
    encoder
        .set_icc_profile(icc.clone())
        .expect("JPEG supports an embedded profile");
    encoder
        .write_image(&pixels, width, height, ExtendedColorType::Rgb8)
        .expect("encode");

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("display-p3.jpg");
    std::fs::create_dir_all(out.parent().expect("parent")).expect("create fixtures dir");
    std::fs::write(&out, &bytes).expect("write jpeg");

    println!(
        "wrote {} ({} bytes, {} of profile)",
        out.display(),
        bytes.len(),
        icc.len()
    );
}
