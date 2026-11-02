/* $lib/presets/presetDocument.ts */

import {
    type Sliders,
    HSL_KEYS,
    SLIDER_TOLERANCE,
    defaultSlidersRGB,
} from "$lib/types/imgParameters";
import { type Curves, CURVE_CHANNELS, cloneCurves, curvesEqual, defaultCurves } from "$lib/types/curves";
import {
    formatAdjustments,
    readCurves,
    readDocument,
    readSliders,
} from "$lib/utils/adjustmentsClipboard";

/** Must match EXTENSION in presets.rs. */
export const PRESET_EXTENSION = "odpreset";

export type PresetMode = "lighter-room" | "darkroom" | "both";

/** A set of controls a preset includes or leaves out as one. */
export interface PresetGroup {
    id: string;
    label: string;
    mode: PresetMode;
    keys: (keyof Sliders)[];
    curves?: boolean;
}

// Every slider belongs to exactly one group; tests/unit/presets.test.ts holds
// the list to that.
export const PRESET_GROUPS: PresetGroup[] = [
    { id: "whiteBalance", label: "White Balance", mode: "lighter-room", keys: ["wbTemp", "wbTint"] },
    { id: "light", label: "Light", mode: "lighter-room", keys: ["exposure", "contrast", "brightness"] },
    { id: "tone", label: "Tone", mode: "lighter-room", keys: ["highlights", "shadows", "whites", "blacks"] },
    { id: "colour", label: "Colour", mode: "lighter-room", keys: ["vibrance", "saturation", "hue"] },
    { id: "colourMixer", label: "Colour Mixer", mode: "lighter-room", keys: [...HSL_KEYS] },
    { id: "curves", label: "Curves", mode: "lighter-room", keys: [], curves: true },
    { id: "sharpness", label: "Sharpness", mode: "lighter-room", keys: ["clarity", "texture"] },
    {
        id: "unsharpMask",
        label: "Unsharp Mask",
        mode: "both",
        keys: ["usmAmount", "usmRadius", "usmLumaThreshold", "usmDetailThreshold"],
    },
    { id: "invert", label: "Invert", mode: "darkroom", keys: ["invert"] },
    {
        id: "channelInput",
        label: "Channel Input",
        mode: "darkroom",
        keys: [
            "redBlackPoint",
            "redGamma",
            "redWhitePoint",
            "greenBlackPoint",
            "greenGamma",
            "greenWhitePoint",
            "blueBlackPoint",
            "blueGamma",
            "blueWhitePoint",
        ],
    },
    {
        id: "rgbInput",
        label: "RGB Input",
        mode: "darkroom",
        keys: ["masterBlackPoint", "masterGamma", "masterWhitePoint"],
    },
    { id: "rgbOutput", label: "RGB Output", mode: "darkroom", keys: ["rgbOutputBlack", "rgbOutputWhite"] },
];

/** The adjustments a preset carries, before it has a place in the list. */
export interface PresetContent {
    name: string;
    sliders: Partial<Sliders>;
    curves?: Partial<Curves>;
}

export interface Preset extends PresetContent {
    id: string;
}

const SLIDER_KEYS = Object.keys(defaultSlidersRGB) as (keyof Sliders)[];

/** One line, trimmed, so it survives the document's one-value-per-line form. */
export function cleanPresetName(name: string): string {
    return name.replace(/[\r\n]+/g, " ").trim();
}

/** The groups a preset holds values for. */
export function presetGroupIds(preset: PresetContent): Set<string> {
    const ids = new Set<string>();
    for (const group of PRESET_GROUPS) {
        const hasKey = group.keys.some((key) => preset.sliders[key] !== undefined);
        const hasCurves = group.curves && preset.curves && Object.keys(preset.curves).length > 0;
        if (hasKey || hasCurves) ids.add(group.id);
    }
    return ids;
}

/** The current value of every control in the given groups. */
export function captureGroups(
    groupIds: Iterable<string>,
    sliders: Sliders,
    curves: Curves,
): Pick<PresetContent, "sliders" | "curves"> {
    const wanted = new Set(groupIds);
    const captured: Partial<Sliders> = {};
    let capturedCurves: Curves | undefined;

    for (const group of PRESET_GROUPS) {
        if (!wanted.has(group.id)) continue;
        for (const key of group.keys) (captured as any)[key] = sliders[key];
        if (group.curves) capturedCurves = cloneCurves(curves);
    }

    return capturedCurves ? { sliders: captured, curves: capturedCurves } : { sliders: captured };
}

/**
 * A preset narrowed or widened to `groupIds`: groups it already held keep
 * their stored values, and groups new to it take `current`'s.
 */
export function revisePreset(
    stored: PresetContent,
    current: Pick<PresetContent, "sliders" | "curves">,
    groupIds: Set<string>,
): Pick<PresetContent, "sliders" | "curves"> {
    const had = presetGroupIds(stored);
    const sliders: Partial<Sliders> = {};
    let curves: Partial<Curves> | undefined;

    for (const group of PRESET_GROUPS) {
        if (!groupIds.has(group.id)) continue;
        const source = had.has(group.id) ? stored : current;
        for (const key of group.keys) {
            if (source.sliders[key] !== undefined) (sliders as any)[key] = source.sliders[key];
        }
        if (group.curves) curves = source.curves;
    }

    return curves ? { sliders, curves } : { sliders };
}

/** Groups with at least one control away from its default. */
export function changedGroupIds(sliders: Sliders, curves: Curves): Set<string> {
    const ids = new Set<string>();
    for (const group of PRESET_GROUPS) {
        const keyChanged = group.keys.some((key) => {
            const def = defaultSlidersRGB[key];
            const val = sliders[key];
            if (typeof def === "number" && typeof val === "number") {
                return Math.abs(val - def) > SLIDER_TOLERANCE;
            }
            return val !== def;
        });
        const curvesChanged = group.curves && !curvesEqual(curves, defaultCurves());
        if (keyChanged || curvesChanged) ids.add(group.id);
    }
    return ids;
}

export function serializePreset(preset: PresetContent): string {
    const lines = [
        "# Open Darkroom preset",
        `name: "${cleanPresetName(preset.name)}"`,
        ...formatAdjustments(preset.sliders, SLIDER_KEYS, preset.curves),
    ];
    return `${lines.join("\n")}\n`;
}

/**
 * Reads a preset document, or any adjustments document the clipboard copy
 * writes. One without a name takes `fallbackName`.
 */
export function parsePreset(text: string, fallbackName = "Untitled preset"): PresetContent {
    const entries = readDocument(text);
    const sliders = readSliders(entries, SLIDER_KEYS);
    const curves = readCurves(entries);
    const hasCurves = CURVE_CHANNELS.some((channel) => curves[channel]);

    if (Object.keys(sliders).length === 0 && !hasCurves) {
        throw new Error("The document holds no adjustments");
    }

    const name = cleanPresetName(entries.get("name") ?? "") || cleanPresetName(fallbackName);
    return hasCurves ? { name, sliders, curves } : { name, sliders };
}
