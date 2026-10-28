// src-native/src/shaders.rs
//
// Every WGSL file the app runs, flattened by build.rs from the same sources
// the Tauri frontend compiles.

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
flattened!(HISTOGRAM_SCALE_WGSL, "histogramScale.wgsl");
flattened!(HISTOGRAM_DRAW_WGSL, "histogramDraw.wgsl");
flattened!(VIEW_SAMPLE_WGSL, "viewSample.wgsl");
flattened!(MIPMAP_WGSL, "mipmap.wgsl");
flattened!(COLOR_SPACE_WGSL, "colorSpace.wgsl");
flattened!(COLOR_SPACE_SCALE_WGSL, "colorSpaceScale.wgsl");
flattened!(CHROMATICITY_DRAW_WGSL, "chromaticityDraw.wgsl");
flattened!(COLOR_VOLUME_DRAW_WGSL, "colorVolumeDraw.wgsl");

/// The window chrome: glass, shadows, the scrim and the image composite.
pub const CHROME_WGSL: &str = include_str!("gpu/chrome.wgsl");
pub const BLUR_WGSL: &str = include_str!("gpu/blur.wgsl");
