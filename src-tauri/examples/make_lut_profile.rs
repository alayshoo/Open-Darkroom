// src-tauri/examples/make_lut_profile.rs
//
// Generates tests/fixtures/srgb-lut.icc, the LUT profile `SourceSpace::bake`
// exists for. Run with:
//
//     cargo run --example make_lut_profile
//
// A scanner's profile describes its colour with a lookup table in an A2B0 tag
// rather than with three curves and a 3x3 matrix, because a sensor is not
// well-behaved enough for a formula to fit it. That is the shape `decompose`
// rejects and `bake` converts on the CPU, and the one no test could reach:
// lcms2's safe API cannot build an A2B0 pipeline from stages, since a
// `Pipeline` is only ever created empty and concatenated.
//
// It can be borrowed, though. A device link holds exactly such a pipeline, so
// this builds one for sRGB -> Lab and rehouses its pipeline in an ordinary
// input profile. The result is a real LUT profile whose conversion is sRGB's,
// which is what lets a test compare the baked path against the matrix path
// instead of against a restatement of itself.

use std::path::PathBuf;

use lcms2::{
    CIExyY, ColorSpaceSignature, Flags, Intent, PixelFormat, Profile, ProfileClassSignature, Tag,
    TagSignature, Transform,
};

/// The ICC profile connection space white, which a Lab PCS is defined against.
const D50: CIExyY = CIExyY {
    x: 0.3457,
    y: 0.3585,
    Y: 1.0,
};

fn main() {
    let lab = Profile::new_lab4_context(lcms2::GlobalContext::new(), &D50).expect("lab profile");

    // sRGB in, Lab out: the conversion the fixture is meant to describe.
    let transform: Transform<[u16; 3], [u16; 3]> = Transform::new(
        &Profile::new_srgb(),
        PixelFormat::RGB_16,
        &lab,
        PixelFormat::Lab_16,
        Intent::RelativeColorimetric,
    )
    .expect("srgb -> lab transform");

    // The link exists only to hold the pipeline; its own header is wrong for an
    // input profile, which is why the pipeline is moved rather than the file
    // being used as it stands.
    //
    // A coarse grid on purpose: this is a fixture committed to the repository,
    // and the default precalculation writes a 49-node CLUT at 216 KB for no
    // accuracy a test can use.
    let link =
        Profile::new_device_link(&transform, 4.3, Flags::LOWRES_PRECALC).expect("device link");
    let pipeline = match link.read_tag(TagSignature::AToB0Tag) {
        Tag::Pipeline(p) => p,
        other => panic!("the device link carries no A2B0 pipeline: {other:?}"),
    };

    let mut profile = Profile::new_placeholder();
    profile.set_device_class(ProfileClassSignature::InputClass);
    profile.set_color_space(ColorSpaceSignature::RgbData);
    profile.set_pcs(ColorSpaceSignature::LabData);
    profile.set_version(4.3);

    assert!(
        profile.write_tag(TagSignature::AToB0Tag, Tag::Pipeline(pipeline)),
        "could not write the A2B0 tag"
    );
    assert!(
        profile.write_tag(TagSignature::MediaWhitePointTag, Tag::CIEXYZ(&D50.into())),
        "could not write the white point"
    );

    let bytes = profile.icc().expect("serialise");

    // Written where the tests read it from, relative to the crate root rather
    // than the working directory, so it lands in the same place either way.
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("srgb-lut.icc");
    std::fs::create_dir_all(out.parent().expect("parent")).expect("create fixtures dir");
    std::fs::write(&out, &bytes).expect("write profile");

    println!("wrote {} ({} bytes)", out.display(), bytes.len());
}
