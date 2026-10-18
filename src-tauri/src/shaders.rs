// src-tauri/src/shaders.rs
//
// The frontend's WGSL, with every `//!include` already resolved. build.rs
// flattens the shaders into OUT_DIR, so these are the same sources the browser
// compiles rather than the files on disk.

macro_rules! flattened {
    ($name:ident, $file:literal) => {
        pub const $name: &str = include_str!(concat!(env!("OUT_DIR"), "/shaders/", $file));
    };
}

flattened!(CALC_PARAMS_WGSL, "calcParams.wgsl");
flattened!(DEVELOP_WGSL, "develop.wgsl");
flattened!(CALC_SHARP_TEXTURE_WGSL, "calcSharpTexture.wgsl");
flattened!(COMPOSITE_WGSL, "composite.wgsl");
flattened!(COLOR_SPACE_ENCODE_WGSL, "colorSpaceEncode.wgsl");
flattened!(HISTOGRAM_WGSL, "histogram.wgsl");

// Not compiled by the export path — these are the frontend's colour-space
// tools. They are flattened here so the drift guards in shader_contracts.rs can
// read them: they run the same develop chain the export does, and a chain that
// drifts is exactly what those guards exist to catch.
flattened!(COLOR_SPACE_WGSL, "colorSpace.wgsl");
flattened!(COLOR_SPACE_SCALE_WGSL, "colorSpaceScale.wgsl");
flattened!(CHROMATICITY_DRAW_WGSL, "chromaticityDraw.wgsl");
flattened!(COLOR_VOLUME_DRAW_WGSL, "colorVolumeDraw.wgsl");
