// $lib/types/imgParameters.ts

// Sliders is the type used in the UI
export interface Sliders extends Record<HslKey, number> {
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

const HSL_NEUTRAL = Object.fromEntries(HSL_KEYS.map((key) => [key, 0])) as Record<HslKey, number>;

/** Below this a numeric slider counts as sitting on its target value. */
export const SLIDER_TOLERANCE = 1e-6;

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
    ...HSL_NEUTRAL,
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
    ...HSL_NEUTRAL,
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
