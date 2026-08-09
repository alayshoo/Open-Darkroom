// $lib/types/imgParameters.ts

// Sliders is the type used in the UI
export interface Sliders {
    invert: boolean;
    redBlackPoint: number;
    greenBlackPoint: number;
    blueBlackPoint: number;
    redGamma: number;
    greenGamma: number;
    blueGamma: number;
    redWhitePoint: number;
    greenWhitePoint: number;
    blueWhitePoint: number;
    rgbOutputBlack: number;
    rgbOutputWhite: number;
    wbTemp: number;
    wbTint: number;
    exposure: number;
    contrast: number;
    brightness: number;
    highlights: number;
    shadows: number;
    whites: number;
    blacks: number;
    saturation: number;
    vibrance: number;
    hue: number;
    clarity: number;
    texture: number;
    usmAmount: number;
    usmRadius: number;
    usmLumaThreshold: number;
    usmDetailThreshold: number;
}

// These are the default values of the slider values in RGB mode
export const defaultSlidersRGB: Sliders = {
    invert: false,
    redBlackPoint: 0,
    greenBlackPoint: 0,
    blueBlackPoint: 0,
    redGamma: 1.0,
    greenGamma: 1.0,
    blueGamma: 1.0,
    redWhitePoint: 255,
    greenWhitePoint: 255,
    blueWhitePoint: 255,
    rgbOutputBlack: 0,
    rgbOutputWhite: 255,
    wbTemp: 5500,
    wbTint: 0,
    exposure: 0,
    contrast: 0,
    brightness: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    hue: 0,
    saturation: 0,
    vibrance: 0,
    clarity: 0,
    texture: 0,
    usmAmount: 0,
    // Radius has no neutral value of its own — the mask is switched off by
    // amount 0, so this is simply the starting kernel width.
    usmRadius: 1.0,
    usmLumaThreshold: 0,
    usmDetailThreshold: 0,
};

// These are the default values of the slider values in BW mode
export const defaultSlidersBW: Sliders = {
    invert: false,
    redBlackPoint: 0,
    greenBlackPoint: 0,
    blueBlackPoint: 0,
    redGamma: 1.0,
    greenGamma: 1.0,
    blueGamma: 1.0,
    redWhitePoint: 255,
    greenWhitePoint: 255,
    blueWhitePoint: 255,
    rgbOutputBlack: 0,
    rgbOutputWhite: 255,
    wbTemp: 5500,
    wbTint: 0,
    exposure: 0,
    contrast: 0,
    brightness: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    hue: 0,
    saturation: -100,
    vibrance: 0,
    clarity: 0,
    texture: 0,
    usmAmount: 0,
    usmRadius: 1.0,
    usmLumaThreshold: 0,
    usmDetailThreshold: 0,
};

export const overridesBW: Partial<Sliders> = {
    saturation: -100,
    vibrance: 0,
    wbTemp: 5500,
    wbTint: 0,
};
