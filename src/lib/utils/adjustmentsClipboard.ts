/* $lib/utils/adjustmentsClipboard.ts */

import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { type Sliders, defaultSlidersRGB } from "$lib/types/imgParameters";
import { CURVE_CHANNELS, type CurveChannel, type CurvePoint, type Curves } from "$lib/types/curves";

/* The document is a flat map of scalars — no nesting, no sequences, no
   multi-line strings — so it is written and read directly rather than through a
   YAML library. Reading is the lenient half: it takes comments, document
   markers, quotes and stray whitespace, since the point of the format is that
   the file can be edited by hand between a copy and a paste. */

function formatValue(value: number | boolean): string {
    if (typeof value === "boolean") return value ? "true" : "false";
    // Drops the float noise animated values pick up, well below any slider's
    // own step.
    return String(Number(value.toFixed(6)));
}

// Each curve is one line of space-separated `x,y` pairs.
function curveKey(channel: CurveChannel): string {
    return `curve${channel[0].toUpperCase()}${channel.slice(1)}`;
}

function formatCurve(points: CurvePoint[]): string {
    return points.map((p) => `${formatValue(p.x)},${formatValue(p.y)}`).join(" ");
}

// Callers pass the keys their mode owns, so a document never carries — or
// applies — adjustments belonging to the other mode. Curves are only passed
// by the mode that owns them.
export async function copyAdjustments(
    sliders: Sliders,
    keys: (keyof Sliders)[],
    curves?: Curves,
): Promise<void> {
    const lines = keys.map((key) => `${key}: ${formatValue(sliders[key])}`);
    if (curves) {
        for (const channel of CURVE_CHANNELS) {
            lines.push(`${curveKey(channel)}: ${formatCurve(curves[channel])}`);
        }
    }
    await writeText(`${lines.join("\n")}\n`);
}

function readDocument(text: string): Map<string, string> {
    const entries = new Map<string, string>();

    for (const rawLine of text.split(/\r?\n/)) {
        const line = rawLine.trim();
        if (!line || line.startsWith("#") || line === "---" || line === "...") {
            continue;
        }

        const colon = line.indexOf(":");
        if (colon < 0) continue;

        const key = line.slice(0, colon).trim();
        let value = line.slice(colon + 1).trim();

        const comment = value.indexOf(" #");
        if (comment >= 0) value = value.slice(0, comment).trim();

        const first = value[0];
        if (
            value.length >= 2 &&
            (first === '"' || first === "'") &&
            value.endsWith(first)
        ) {
            value = value.slice(1, -1);
        }

        if (key) entries.set(key, value);
    }

    return entries;
}

function readSliders(entries: Map<string, string>, keys: (keyof Sliders)[]): Partial<Sliders> {
    const values: Partial<Sliders> = {};

    for (const key of keys) {
        const raw = entries.get(key);
        if (raw === undefined || raw === "") continue;

        if (typeof defaultSlidersRGB[key] === "boolean") {
            const flag = raw.toLowerCase();
            if (flag === "true" || flag === "false") {
                (values as any)[key] = flag === "true";
            }
        } else {
            const num = Number(raw);
            if (Number.isFinite(num)) (values as any)[key] = num;
        }
    }

    return values;
}

// A curve that is malformed — out of order, off either end, or not numbers —
// is skipped whole rather than repaired into something the user did not write.
function parseCurve(raw: string): CurvePoint[] | null {
    const points: CurvePoint[] = [];
    for (const pair of raw.split(/\s+/)) {
        const [x, y] = pair.split(",").map(Number);
        if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
        if (y < 0 || y > 1) return null;
        if (points.length > 0 && x <= points[points.length - 1].x) return null;
        points.push({ x, y });
    }
    if (points.length < 2 || points[0].x !== 0 || points[points.length - 1].x !== 1) {
        return null;
    }
    return points;
}

function readCurves(entries: Map<string, string>): Partial<Curves> {
    const curves: Partial<Curves> = {};
    for (const channel of CURVE_CHANNELS) {
        const raw = entries.get(curveKey(channel));
        if (!raw) continue;
        const points = parseCurve(raw);
        if (points) curves[channel] = points;
    }
    return curves;
}

function readMapping(text: string): Map<string, string> {
    const entries = readDocument(text);
    if (entries.size === 0) {
        throw new Error("Clipboard does not contain a YAML mapping");
    }
    return entries;
}

// Anything outside `keys` is dropped rather than rejected, so a document
// written by an older or newer build still applies whatever the two share.
export function parseAdjustments(
    text: string,
    keys: (keyof Sliders)[],
): Partial<Sliders> {
    const values = readSliders(readMapping(text), keys);
    if (Object.keys(values).length === 0) {
        throw new Error("Clipboard YAML holds no adjustments for this mode");
    }

    return values;
}

/** As parseAdjustments, with the curves read alongside when `withCurves` is set. */
export function parseAdjustmentSet(
    text: string,
    keys: (keyof Sliders)[],
    withCurves: boolean,
): { sliders: Partial<Sliders>; curves: Partial<Curves> } {
    const entries = readMapping(text);
    const sliders = readSliders(entries, keys);
    const curves = withCurves ? readCurves(entries) : {};

    if (Object.keys(sliders).length === 0 && Object.keys(curves).length === 0) {
        throw new Error("Clipboard YAML holds no adjustments for this mode");
    }

    return { sliders, curves };
}

export async function readAdjustments(
    keys: (keyof Sliders)[],
    withCurves: boolean,
): Promise<{ sliders: Partial<Sliders>; curves: Partial<Curves> }> {
    return parseAdjustmentSet((await readText()) ?? "", keys, withCurves);
}
