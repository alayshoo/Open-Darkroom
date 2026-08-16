/* $lib/utils/adjustmentsClipboard.ts */

import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { type Sliders, defaultSlidersRGB } from "$lib/types/imgParameters";

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

// Callers pass the keys their mode owns, so a document never carries — or
// applies — adjustments belonging to the other mode.
export async function copyAdjustments(
    sliders: Sliders,
    keys: (keyof Sliders)[],
): Promise<void> {
    const lines = keys.map((key) => `${key}: ${formatValue(sliders[key])}`);
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

// Anything outside `keys` is dropped rather than rejected, so a document
// written by an older or newer build still applies whatever the two share.
export function parseAdjustments(
    text: string,
    keys: (keyof Sliders)[],
): Partial<Sliders> {
    const entries = readDocument(text);
    if (entries.size === 0) {
        throw new Error("Clipboard does not contain a YAML mapping");
    }

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

    if (Object.keys(values).length === 0) {
        throw new Error("Clipboard YAML holds no adjustments for this mode");
    }

    return values;
}

export async function readAdjustments(
    keys: (keyof Sliders)[],
): Promise<Partial<Sliders>> {
    return parseAdjustments((await readText()) ?? "", keys);
}
