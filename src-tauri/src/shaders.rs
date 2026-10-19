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

// Frontend-only colour-space tools, flattened so shader_contracts.rs can check
// them against the shared develop chain.
flattened!(COLOR_SPACE_WGSL, "colorSpace.wgsl");
flattened!(COLOR_SPACE_SCALE_WGSL, "colorSpaceScale.wgsl");
flattened!(CHROMATICITY_DRAW_WGSL, "chromaticityDraw.wgsl");
flattened!(COLOR_VOLUME_DRAW_WGSL, "colorVolumeDraw.wgsl");
