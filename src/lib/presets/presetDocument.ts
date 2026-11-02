/* $lib/presets/presetDocument.ts */

import {
    type Sliders,
    HSL_BANDS,
    SLIDER_TOLERANCE,
    defaultSlidersRGB,
    hslKey,
} from "$lib/types/imgParameters";
import {
    type CurveChannel,
    type Curves,
    CURVE_CHANNELS,
    isIdentityCurve,
} from "$lib/types/curves";
import {
    formatAdjustments,
    readCurves,
    readDocument,
    readSliders,
} from "$lib/utils/adjustmentsClipboard";

/** Must match EXTENSION in presets.rs. */
export const PRESET_EXTENSION = "odpreset";

export type PresetMode = "lighter-room" | "darkroom" | "both";

/** One checkbox in the preset dialog: a slider key, or `curve:<channel>`. */
export interface PresetField {
    id: string;
    label: string;
}

/** Fields laid out on one line under a label of their own, like a colour band. */
export interface PresetRow {
    label: string;
    fields: PresetField[];
}

export interface PresetGroup {
    id: string;
    label: string;
    mode: PresetMode;
    fields: PresetField[];
    rows?: PresetRow[];
}

const CURVE_PREFIX = "curve:";

function field(id: string, label: string): PresetField {
    return { id, label };
}

function rowsGroup(id: string, label: string, mode: PresetMode, rows: PresetRow[]): PresetGroup {
    return { id, label, mode, rows, fields: rows.flatMap((row) => row.fields) };
}

const capitalise = (word: string) => word[0].toUpperCase() + word.slice(1);

const channelRow = (channel: "red" | "green" | "blue"): PresetRow => ({
    label: capitalise(channel),
    fields: [
        field(`${channel}BlackPoint`, "Black"),
        field(`${channel}Gamma`, "Gamma"),
        field(`${channel}WhitePoint`, "White"),
    ],
});

const bandRow = (band: (typeof HSL_BANDS)[number]): PresetRow => ({
    label: capitalise(band),
    fields: [
        field(hslKey(band, "Hue"), "Hue"),
        field(hslKey(band, "Sat"), "Sat"),
        field(hslKey(band, "Lum"), "Lum"),
    ],
});

// Every slider and curve channel is exactly one field; tests/unit/presets.test.ts
// holds the list to that.
export const PRESET_GROUPS: PresetGroup[] = [
    {
        id: "whiteBalance",
        label: "White Balance",
        mode: "lighter-room",
        fields: [field("wbTemp", "Temperature"), field("wbTint", "Tint")],
    },
    {
        id: "light",
        label: "Light",
        mode: "lighter-room",
        fields: [field("exposure", "Exposure"), field("contrast", "Contrast"), field("brightness", "Brightness")],
    },
    {
        id: "tone",
        label: "Tone",
        mode: "lighter-room",
        fields: [
            field("highlights", "Highlights"),
            field("shadows", "Shadows"),
            field("whites", "Whites"),
            field("blacks", "Blacks"),
        ],
    },
    {
        id: "colour",
        label: "Colour",
        mode: "lighter-room",
        fields: [field("vibrance", "Vibrance"), field("saturation", "Saturation"), field("hue", "Hue")],
    },
    {
        id: "curves",
        label: "Curves",
        mode: "lighter-room",
        fields: CURVE_CHANNELS.map((channel) => field(`${CURVE_PREFIX}${channel}`, capitalise(channel))),
    },
    {
        id: "sharpness",
        label: "Sharpness",
        mode: "lighter-room",
        fields: [field("clarity", "Clarity"), field("texture", "Texture")],
    },
    rowsGroup("colourMixer", "Colour Mixer", "lighter-room", HSL_BANDS.map(bandRow)),
    {
        id: "unsharpMask",
        label: "Unsharp Mask",
        mode: "both",
        fields: [
            field("usmAmount", "Amount"),
            field("usmRadius", "Radius"),
            field("usmLumaThreshold", "Luma Threshold"),
            field("usmDetailThreshold", "Detail Threshold"),
        ],
    },
    { id: "invert", label: "Invert", mode: "darkroom", fields: [field("invert", "Invert")] },
    rowsGroup("channelInput", "Channel Input", "darkroom", [
        channelRow("red"),
        channelRow("green"),
        channelRow("blue"),
    ]),
    {
        id: "rgbInput",
        label: "RGB Input",
        mode: "darkroom",
        fields: [
            field("masterBlackPoint", "Black"),
            field("masterGamma", "Gamma"),
            field("masterWhitePoint", "White"),
        ],
    },
    {
        id: "rgbOutput",
        label: "RGB Output",
        mode: "darkroom",
        fields: [field("rgbOutputBlack", "Black"), field("rgbOutputWhite", "White")],
    },
];

export const PRESET_FIELDS: PresetField[] = PRESET_GROUPS.flatMap((group) => group.fields);

/** The curve channel a field id names, or null for a slider. */
export function fieldCurve(id: string): CurveChannel | null {
    if (!id.startsWith(CURVE_PREFIX)) return null;
    const channel = id.slice(CURVE_PREFIX.length) as CurveChannel;
    return CURVE_CHANNELS.includes(channel) ? channel : null;
}

/** The field ids of every group in `mode`, counting the ones both modes share. */
export function modeFieldIds(mode: Exclude<PresetMode, "both">): Set<string> {
    const groups = PRESET_GROUPS.filter((g) => g.mode === mode || g.mode === "both");
    return new Set(groups.flatMap((g) => g.fields.map((f) => f.id)));
}

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

/** The fields a preset holds values for. */
export function presetFieldIds(preset: PresetContent): Set<string> {
    const ids = new Set<string>();
    for (const { id } of PRESET_FIELDS) {
        const channel = fieldCurve(id);
        const held = channel
            ? preset.curves?.[channel] !== undefined
            : preset.sliders[id as keyof Sliders] !== undefined;
        if (held) ids.add(id);
    }
    return ids;
}

/** The values of the given fields, taken from `sliders` and `curves`. */
export function captureFields(
    fieldIds: Iterable<string>,
    sliders: Partial<Sliders>,
    curves: Partial<Curves> = {},
): Pick<PresetContent, "sliders" | "curves"> {
    const captured: Partial<Sliders> = {};
    const capturedCurves: Partial<Curves> = {};

    for (const id of fieldIds) {
        const channel = fieldCurve(id);
        if (channel) {
            const points = curves[channel];
            if (points) capturedCurves[channel] = points.map((p) => ({ ...p }));
        } else if (id in defaultSlidersRGB) {
            const value = sliders[id as keyof Sliders];
            if (value !== undefined) (captured as any)[id] = value;
        }
    }

    return Object.keys(capturedCurves).length > 0
        ? { sliders: captured, curves: capturedCurves }
        : { sliders: captured };
}

/**
 * A preset narrowed or widened to `fieldIds`: fields it already held keep
 * their stored values, and fields new to it take `current`'s.
 */
export function revisePreset(
    stored: PresetContent,
    current: Pick<PresetContent, "sliders" | "curves">,
    fieldIds: Set<string>,
): Pick<PresetContent, "sliders" | "curves"> {
    const held = presetFieldIds(stored);
    const kept = captureFields([...fieldIds].filter((id) => held.has(id)), stored.sliders, stored.curves);
    const added = captureFields([...fieldIds].filter((id) => !held.has(id)), current.sliders, current.curves);
    const sliders = { ...added.sliders, ...kept.sliders };
    const curves = { ...added.curves, ...kept.curves };
    return Object.keys(curves).length > 0 ? { sliders, curves } : { sliders };
}

/** Fields away from their defaults. */
export function changedFieldIds(sliders: Sliders, curves: Curves): Set<string> {
    const ids = new Set<string>();
    for (const { id } of PRESET_FIELDS) {
        const channel = fieldCurve(id);
        if (channel) {
            if (!isIdentityCurve(curves[channel])) ids.add(id);
            continue;
        }
        const def = defaultSlidersRGB[id as keyof Sliders];
        const val = sliders[id as keyof Sliders];
        const changed =
            typeof def === "number" && typeof val === "number"
                ? Math.abs(val - def) > SLIDER_TOLERANCE
                : val !== def;
        if (changed) ids.add(id);
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
