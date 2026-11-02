// $lib/types/cropAspect.ts
//
// The crop tool's aspect lock. It only shapes the crop box while it is drawn;
// the crop itself stores no ratio.

import { fitCrop, turnedSize, type Crop } from "./crop";

export type AspectPresetId = "free" | "original" | "1:1" | "3:2" | "4:3" | "5:4" | "16:9" | "custom";

export interface AspectPreset {
    id: AspectPresetId;
    label: string;
    /** Long side over short side. Absent for Free, Original and Custom, which take theirs elsewhere. */
    ratio?: number;
}

export const ASPECT_PRESETS: AspectPreset[] = [
    { id: "free", label: "Free" },
    { id: "original", label: "Original" },
    { id: "1:1", label: "1:1", ratio: 1 },
    { id: "3:2", label: "3:2", ratio: 3 / 2 },
    { id: "4:3", label: "4:3", ratio: 4 / 3 },
    { id: "5:4", label: "5:4", ratio: 5 / 4 },
    { id: "16:9", label: "16:9", ratio: 16 / 9 },
    { id: "custom", label: "Custom…" },
];

/** Widest a custom ratio may be, either way round. */
export const MAX_CUSTOM_RATIO = 20;

export interface AspectLock {
    preset: AspectPresetId;
    /** Taller than wide on screen. */
    portrait: boolean;
    /** The sides entered for a custom ratio, as typed. */
    custom?: [number, number];
}

export const FREE_ASPECT: AspectLock = { preset: "free", portrait: false };

// Ratios this close are taken as the same.
const ASPECT_TOLERANCE = 1e-3;

/** The lock's width over height, in pixels, or null when free. */
export function aspectRatio(lock: AspectLock, width: number, height: number): number | null {
    if (lock.preset === "free") return null;
    const longOver = ([a, b]: [number, number]) => Math.max(a, b) / Math.min(a, b);
    const long =
        lock.preset === "original"
            ? longOver([width, height])
            : lock.preset === "custom"
              ? longOver(lock.custom ?? [1, 1])
              : (ASPECT_PRESETS.find((p) => p.id === lock.preset)?.ratio ?? 1);
    return lock.portrait ? 1 / long : long;
}

/** Whether two sides make a usable custom ratio. */
export function validCustom(w: number, h: number): boolean {
    if (!(Number.isFinite(w) && Number.isFinite(h) && w > 0 && h > 0)) return false;
    return Math.max(w, h) / Math.min(w, h) <= MAX_CUSTOM_RATIO;
}

/** How a lock reads on screen, e.g. "3:2", or "4:5" when it stands portrait. */
export function aspectLabel(lock: AspectLock): string {
    if (lock.preset === "custom" && lock.custom) {
        const [a, b] = lock.custom;
        const [long, short] = [Math.max(a, b), Math.min(a, b)];
        return lock.portrait ? `${short}:${long}` : `${long}:${short}`;
    }
    const preset = ASPECT_PRESETS.find((p) => p.id === lock.preset);
    if (!preset?.ratio || !lock.portrait) return preset?.label ?? "Free";
    return preset.label.split(":").reverse().join(":");
}

/** The crop's width over height, in pixels. */
export function cropAspect(c: Crop, width: number, height: number): number {
    const [w, h] = turnedSize(c, width, height);
    return (c.w * w) / (c.h * h);
}

export function aspectsMatch(a: number, b: number): boolean {
    return Math.abs(Math.log(a / b)) < ASPECT_TOLERANCE;
}

/**
 * The largest crop of the given aspect that fits the frame, kept as near the
 * crop's centre as the frame allows, then fitted to the image.
 */
export function cropWithAspect(c: Crop, aspect: number, width: number, height: number): Crop {
    const [tw, th] = turnedSize(c, width, height);
    const k = (aspect * th) / tw;
    const w = Math.min(1, k);
    const h = w / k;
    const clamp = (v: number, half: number) => Math.min(Math.max(v, half), 1 - half);
    return fitCrop({ ...c, cx: clamp(c.cx, w / 2), cy: clamp(c.cy, h / 2), w, h }, width, height);
}
