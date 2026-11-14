// $lib/types/editHistory.ts
//
// The undo history of one image, stored with its edit so it is still there
// when the image is opened again. Read strictly: a history with any step this
// build cannot read is dropped whole, since undoing past a gap would be wrong.

import type { Action } from "./historyActions";
import { defaultSlidersRGB, type Sliders } from "./imgParameters";
import { readCrop, readCurveValues, readSliderValues } from "./imageEdit";
import { readRetouch, type Retouch } from "./retouch";

export const EDIT_HISTORY_VERSION = 1;

/** The most steps kept with an edit. Beyond it the oldest go first. */
export const MAX_STORED_STEPS = 200;

export interface EditHistory {
    steps: Action[];
    /** The last step applied, or -1 when every step is undone. */
    pointer: number;
}

/** At most MAX_STORED_STEPS, keeping the step the edit is on. Redo steps go before older ones. */
export function trimEditHistory({ steps, pointer }: EditHistory, max = MAX_STORED_STEPS): EditHistory {
    const excess = steps.length - max;
    if (excess <= 0) return { steps, pointer };
    const future = Math.min(excess, steps.length - 1 - pointer);
    const kept = steps.slice(0, steps.length - future);
    const old = kept.length - max;
    return old > 0 ? { steps: kept.slice(old), pointer: pointer - old } : { steps: kept, pointer };
}

export function serializeEditHistory(history: EditHistory): string {
    const { steps, pointer } = trimEditHistory(history);
    return JSON.stringify({ version: EDIT_HISTORY_VERSION, pointer, steps });
}

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);

function isSliderValue(key: keyof Sliders, value: unknown): value is number | boolean {
    return typeof value === typeof defaultSlidersRGB[key] && (typeof value !== "number" || Number.isFinite(value));
}

function readStrokes(value: unknown): Retouch[] | null {
    if (!Array.isArray(value)) return null;
    const strokes = value.map(readRetouch);
    return strokes.every((s) => s !== null) ? (strokes as Retouch[]) : null;
}

function readStep(value: unknown): Action | null {
    if (!isRecord(value)) return null;
    switch (value.type) {
        case "slider": {
            const key = value.key as keyof Sliders;
            if (typeof key !== "string" || !(key in defaultSlidersRGB)) return null;
            if (!isSliderValue(key, value.oldValue) || !isSliderValue(key, value.newValue)) return null;
            return { type: "slider", key, oldValue: value.oldValue, newValue: value.newValue };
        }
        case "sliders": {
            const step: Extract<Action, { type: "sliders" }> = {
                type: "sliders",
                oldValues: readSliderValues(value.oldValues),
                newValues: readSliderValues(value.newValues),
            };
            if (value.oldCurves !== undefined) step.oldCurves = readCurveValues(value.oldCurves);
            if (value.newCurves !== undefined) step.newCurves = readCurveValues(value.newCurves);
            return step;
        }
        case "curves":
            return { type: "curves", oldCurves: readCurveValues(value.oldCurves), newCurves: readCurveValues(value.newCurves) };
        case "crop":
            return { type: "crop", oldValue: readCrop(value.oldValue), newValue: readCrop(value.newValue) };
        case "retouchAdd": {
            const stroke = readRetouch(value.stroke);
            return stroke ? { type: "retouchAdd", stroke } : null;
        }
        case "retouchClear": {
            const strokes = readStrokes(value.strokes);
            return strokes ? { type: "retouchClear", strokes } : null;
        }
        default:
            return null;
    }
}

/** The history read back from `text`, or null when it cannot be used. */
export function parseEditHistory(text: string | null): EditHistory | null {
    if (!text) return null;
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch {
        return null;
    }
    if (!isRecord(value) || !Array.isArray(value.steps) || !Number.isInteger(value.pointer)) return null;
    const steps = value.steps.map(readStep);
    if (steps.some((s) => s === null)) return null;
    const pointer = value.pointer as number;
    if (pointer < -1 || pointer >= steps.length) return null;
    return { steps: steps as Action[], pointer };
}
