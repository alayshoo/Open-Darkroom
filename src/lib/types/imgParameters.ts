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
    hslRedHue: number;
    hslRedSat: number;
    hslRedLum: number;
    hslOrangeHue: number;
    hslOrangeSat: number;
    hslOrangeLum: number;
    hslYellowHue: number;
    hslYellowSat: number;
    hslYellowLum: number;
    hslGreenHue: number;
    hslGreenSat: number;
    hslGreenLum: number;
    hslAquaHue: number;
    hslAquaSat: number;
    hslAquaLum: number;
    hslBlueHue: number;
    hslBlueSat: number;
    hslBlueLum: number;
    hslPurpleHue: number;
    hslPurpleSat: number;
    hslPurpleLum: number;
    hslMagentaHue: number;
    hslMagentaSat: number;
    hslMagentaLum: number;
}

/** The per-colour HSL bands, in hue order. */
export const HSL_BANDS = ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"] as const;
export type HslBand = (typeof HSL_BANDS)[number];
export type HslChannel = "Hue" | "Sat" | "Lum";
export type HslKey = `hsl${Capitalize<HslBand>}${HslChannel}`;

export function hslKey(band: HslBand, channel: HslChannel): HslKey {
    return `hsl${band[0].toUpperCase()}${band.slice(1)}${channel}` as HslKey;
}

export const HSL_KEYS: HslKey[] = HSL_BANDS.flatMap((band) =>
    (["Hue", "Sat", "Lum"] as const).map((channel) => hslKey(band, channel)),
);

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
    hslRedHue: 0,
    hslRedSat: 0,
    hslRedLum: 0,
    hslOrangeHue: 0,
    hslOrangeSat: 0,
    hslOrangeLum: 0,
    hslYellowHue: 0,
    hslYellowSat: 0,
    hslYellowLum: 0,
    hslGreenHue: 0,
    hslGreenSat: 0,
    hslGreenLum: 0,
    hslAquaHue: 0,
    hslAquaSat: 0,
    hslAquaLum: 0,
    hslBlueHue: 0,
    hslBlueSat: 0,
    hslBlueLum: 0,
    hslPurpleHue: 0,
    hslPurpleSat: 0,
    hslPurpleLum: 0,
    hslMagentaHue: 0,
    hslMagentaSat: 0,
    hslMagentaLum: 0,
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
    hslRedHue: 0,
    hslRedSat: 0,
    hslRedLum: 0,
    hslOrangeHue: 0,
    hslOrangeSat: 0,
    hslOrangeLum: 0,
    hslYellowHue: 0,
    hslYellowSat: 0,
    hslYellowLum: 0,
    hslGreenHue: 0,
    hslGreenSat: 0,
    hslGreenLum: 0,
    hslAquaHue: 0,
    hslAquaSat: 0,
    hslAquaLum: 0,
    hslBlueHue: 0,
    hslBlueSat: 0,
    hslBlueLum: 0,
    hslPurpleHue: 0,
    hslPurpleSat: 0,
    hslPurpleLum: 0,
    hslMagentaHue: 0,
    hslMagentaSat: 0,
    hslMagentaLum: 0,
};

/* The two control modes own disjoint halves of the slider set — the darkroom
   pager drives these, every other page drives the rest. Copy, paste and reset
   only ever touch the half belonging to the mode the user is in. */
const darkroomKeys = new Set<keyof Sliders>([
    "invert",
    "redBlackPoint",
    "redGamma",
    "redWhitePoint",
    "greenBlackPoint",
    "greenGamma",
    "greenWhitePoint",
    "blueBlackPoint",
    "blueGamma",
    "blueWhitePoint",
    "rgbOutputBlack",
    "rgbOutputWhite",
]);

export function modeKeys(isDarkroom: boolean): (keyof Sliders)[] {
    return (Object.keys(defaultSlidersRGB) as (keyof Sliders)[]).filter(
        (key) => darkroomKeys.has(key) === isDarkroom,
    );
}

export const overridesBW: Partial<Sliders> = {
    saturation: -100,
    vibrance: 0,
    wbTemp: 5500,
    wbTint: 0,
};
