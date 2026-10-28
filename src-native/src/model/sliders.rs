// src-native/src/model/sliders.rs
//
// The adjustments, as src/lib/types/imgParameters.ts describes them. Numeric
// sliders live in one array indexed by `N`, in the order the TypeScript object
// lists its keys, so copy, paste, reset and history can walk them by name.

use crate::export_rendering::{SlidersPayload, ToneMapPayload};

/// Every numeric slider, in `defaultSlidersRGB` order.
pub const NAMES: [&str; 53] = [
    "redBlackPoint",
    "greenBlackPoint",
    "blueBlackPoint",
    "redGamma",
    "greenGamma",
    "blueGamma",
    "redWhitePoint",
    "greenWhitePoint",
    "blueWhitePoint",
    "rgbOutputBlack",
    "rgbOutputWhite",
    "wbTemp",
    "wbTint",
    "exposure",
    "contrast",
    "brightness",
    "highlights",
    "shadows",
    "whites",
    "blacks",
    "hue",
    "saturation",
    "vibrance",
    "clarity",
    "texture",
    "usmAmount",
    "usmRadius",
    "usmLumaThreshold",
    "usmDetailThreshold",
    "hslRedHue",
    "hslRedSat",
    "hslRedLum",
    "hslOrangeHue",
    "hslOrangeSat",
    "hslOrangeLum",
    "hslYellowHue",
    "hslYellowSat",
    "hslYellowLum",
    "hslGreenHue",
    "hslGreenSat",
    "hslGreenLum",
    "hslAquaHue",
    "hslAquaSat",
    "hslAquaLum",
    "hslBlueHue",
    "hslBlueSat",
    "hslBlueLum",
    "hslPurpleHue",
    "hslPurpleSat",
    "hslPurpleLum",
    "hslMagentaHue",
    "hslMagentaSat",
    "hslMagentaLum",
];

/// Index of each numeric slider in `Sliders::v`.
#[allow(non_upper_case_globals, dead_code)]
pub mod n {
    pub const redBlackPoint: usize = 0;
    pub const greenBlackPoint: usize = 1;
    pub const blueBlackPoint: usize = 2;
    pub const redGamma: usize = 3;
    pub const greenGamma: usize = 4;
    pub const blueGamma: usize = 5;
    pub const redWhitePoint: usize = 6;
    pub const greenWhitePoint: usize = 7;
    pub const blueWhitePoint: usize = 8;
    pub const rgbOutputBlack: usize = 9;
    pub const rgbOutputWhite: usize = 10;
    pub const wbTemp: usize = 11;
    pub const wbTint: usize = 12;
    pub const exposure: usize = 13;
    pub const contrast: usize = 14;
    pub const brightness: usize = 15;
    pub const highlights: usize = 16;
    pub const shadows: usize = 17;
    pub const whites: usize = 18;
    pub const blacks: usize = 19;
    pub const hue: usize = 20;
    pub const saturation: usize = 21;
    pub const vibrance: usize = 22;
    pub const clarity: usize = 23;
    pub const texture: usize = 24;
    pub const usmAmount: usize = 25;
    pub const usmRadius: usize = 26;
    pub const usmLumaThreshold: usize = 27;
    pub const usmDetailThreshold: usize = 28;
    /// The first HSL key; the bands follow in hue order, Hue/Sat/Lum each.
    pub const hsl: usize = 29;
}

pub const COUNT: usize = NAMES.len();

/// Hue, Sat or Lum of one band.
pub fn hsl_key(band: usize, channel: usize) -> usize {
    n::hsl + band * 3 + channel
}

pub fn hsl_keys() -> std::ops::Range<usize> {
    n::hsl..COUNT
}

/// Below this a numeric slider counts as sitting on its target value.
pub const SLIDER_TOLERANCE: f64 = 1e-6;

/// One slider, numeric or the invert switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Invert,
    N(usize),
}

impl Key {
    pub fn name(self) -> &'static str {
        match self {
            Key::Invert => "invert",
            Key::N(i) => NAMES[i],
        }
    }

    /// Every key, in `defaultSlidersRGB` order.
    pub fn all() -> impl Iterator<Item = Key> {
        std::iter::once(Key::Invert).chain((0..COUNT).map(Key::N))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sliders {
    pub invert: bool,
    pub v: [f64; COUNT],
}

impl Sliders {
    pub fn default_rgb() -> Self {
        let mut v = [0.0; COUNT];
        v[n::redGamma] = 1.0;
        v[n::greenGamma] = 1.0;
        v[n::blueGamma] = 1.0;
        v[n::redWhitePoint] = 255.0;
        v[n::greenWhitePoint] = 255.0;
        v[n::blueWhitePoint] = 255.0;
        v[n::rgbOutputWhite] = 255.0;
        v[n::wbTemp] = 5500.0;
        // Radius has no neutral value of its own — the mask is switched off by
        // amount 0, so this is simply the starting kernel width.
        v[n::usmRadius] = 1.0;
        Self { invert: false, v }
    }

    pub fn get(&self, key: Key) -> Value {
        match key {
            Key::Invert => Value::Bool(self.invert),
            Key::N(i) => Value::Num(self.v[i]),
        }
    }

    pub fn set(&mut self, key: Key, value: Value) {
        match (key, value) {
            (Key::Invert, Value::Bool(b)) => self.invert = b,
            (Key::N(i), Value::Num(x)) => self.v[i] = x,
            _ => {}
        }
    }

    /// What the GPU and the export are handed: the same struct the Tauri
    /// frontend sends, so both paths pack it the same way.
    pub fn payload(&self, tone_map: ToneMapPayload, curve_lut: Option<&[f32]>) -> SlidersPayload {
        let f = |i: usize| self.v[i] as f32;
        let h = |band: usize, ch: usize| f(hsl_key(band, ch));
        SlidersPayload {
            invert: self.invert,
            red_black_point: f(n::redBlackPoint),
            green_black_point: f(n::greenBlackPoint),
            blue_black_point: f(n::blueBlackPoint),
            red_white_point: f(n::redWhitePoint),
            green_white_point: f(n::greenWhitePoint),
            blue_white_point: f(n::blueWhitePoint),
            rgb_output_black: f(n::rgbOutputBlack),
            rgb_output_white: f(n::rgbOutputWhite),
            red_gamma: f(n::redGamma),
            green_gamma: f(n::greenGamma),
            blue_gamma: f(n::blueGamma),
            wb_temp: f(n::wbTemp),
            wb_tint: f(n::wbTint),
            exposure: f(n::exposure),
            contrast: f(n::contrast),
            brightness: f(n::brightness),
            highlights: f(n::highlights),
            shadows: f(n::shadows),
            whites: f(n::whites),
            blacks: f(n::blacks),
            saturation: f(n::saturation),
            vibrance: f(n::vibrance),
            hue: f(n::hue),
            clarity: f(n::clarity),
            texture: f(n::texture),
            usm_amount: f(n::usmAmount),
            usm_radius: f(n::usmRadius),
            usm_luma_threshold: f(n::usmLumaThreshold),
            usm_detail_threshold: f(n::usmDetailThreshold),
            hsl_red_hue: h(0, 0),
            hsl_red_sat: h(0, 1),
            hsl_red_lum: h(0, 2),
            hsl_orange_hue: h(1, 0),
            hsl_orange_sat: h(1, 1),
            hsl_orange_lum: h(1, 2),
            hsl_yellow_hue: h(2, 0),
            hsl_yellow_sat: h(2, 1),
            hsl_yellow_lum: h(2, 2),
            hsl_green_hue: h(3, 0),
            hsl_green_sat: h(3, 1),
            hsl_green_lum: h(3, 2),
            hsl_aqua_hue: h(4, 0),
            hsl_aqua_sat: h(4, 1),
            hsl_aqua_lum: h(4, 2),
            hsl_blue_hue: h(5, 0),
            hsl_blue_sat: h(5, 1),
            hsl_blue_lum: h(5, 2),
            hsl_purple_hue: h(6, 0),
            hsl_purple_sat: h(6, 1),
            hsl_purple_lum: h(6, 2),
            hsl_magenta_hue: h(7, 0),
            hsl_magenta_sat: h(7, 1),
            hsl_magenta_lum: h(7, 2),
            tone_map,
            curve_lut: curve_lut.map(<[f32]>::to_vec).unwrap_or_default(),
        }
    }
}

/// The darkroom pager's half of the slider set; every other page drives the
/// rest. Copy, paste and reset only ever touch the mode the user is in.
const DARKROOM_KEYS: [usize; 11] = [
    n::redBlackPoint,
    n::redGamma,
    n::redWhitePoint,
    n::greenBlackPoint,
    n::greenGamma,
    n::greenWhitePoint,
    n::blueBlackPoint,
    n::blueGamma,
    n::blueWhitePoint,
    n::rgbOutputBlack,
    n::rgbOutputWhite,
];

pub fn mode_keys(is_darkroom: bool) -> Vec<Key> {
    Key::all()
        .filter(|&k| {
            let darkroom = match k {
                Key::Invert => true,
                Key::N(i) => DARKROOM_KEYS.contains(&i),
            };
            darkroom == is_darkroom
        })
        .collect()
}

/// What black and white pins, and to what.
pub const OVERRIDES_BW: [(usize, f64); 4] = [
    (n::saturation, -100.0),
    (n::vibrance, 0.0),
    (n::wbTemp, 5500.0),
    (n::wbTint, 0.0),
];

pub fn is_bw_override(i: usize) -> bool {
    OVERRIDES_BW.iter().any(|&(k, _)| k == i)
}
