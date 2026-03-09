// $lib/types/adjustments.ts

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
};

export const overridesBW: Partial<Sliders> = {
    saturation: -100,
    vibrance: 0,
    wbTemp: 5500,
    wbTint: 0,
};

// Adjustments is the type used in the GPU Pipeline
export interface Parameters {  
    darkroomMatrix: Matrix4x4;
    redGamma: number;
    greenGamma: number;
    blueGamma: number;
    wbMatrix: Float32Array<ArrayBuffer>;
    exposure: number;
    contrast: number;
    brightness: number;
    highlights: number;
    shadows: number;
    whites: number;
    blacks: number;
    hueSatMatrix: Matrix4x4;
    vibrance: number;
}


import type { Matrix4x4 } from "$lib/utils/calcMatrixHelpers";
import { calcDarkroomMatrix } from "$lib/utils/calcDarkroomMatrix";
import { calcHueSatMatrix } from "$lib/utils/calcHueSatMatrix";
import { temperatureTintToWBMatrix } from "../utils/calcWBMatrix";

// Maps UI slider values to shader-expected ranges
export function mapSlidersToParameters(sliders: Sliders): Parameters {

    let params = {} as Parameters;

    params.darkroomMatrix = calcDarkroomMatrix(
        sliders.invert,
        sliders.redBlackPoint / 255,
        sliders.greenBlackPoint / 255,
        sliders.blueBlackPoint / 255,
        sliders.redWhitePoint / 255,
        sliders.greenWhitePoint / 255,
        sliders.blueWhitePoint / 255,
        sliders.rgbOutputBlack / 255,
        sliders.rgbOutputWhite / 255,
    );

    params.hueSatMatrix = calcHueSatMatrix(
        1 + sliders.saturation / 100, // 0..2, 1 = identity
        sliders.hue * Math.PI / 180,  // degrees -> radians
    );

    params.redGamma = sliders.redGamma;
    params.greenGamma = sliders.greenGamma;
    params.blueGamma = sliders.blueGamma;
    params.wbMatrix = temperatureTintToWBMatrix(sliders.wbTemp, sliders.wbTint);
    params.exposure = sliders.exposure;
    params.contrast = sliders.contrast / 100;
    params.brightness = sliders.brightness;
    params.highlights = sliders.highlights;
    params.shadows = sliders.shadows;
    params.whites = sliders.whites;
    params.blacks = sliders.blacks;
    params.vibrance = sliders.vibrance / 100;

    return params;
}