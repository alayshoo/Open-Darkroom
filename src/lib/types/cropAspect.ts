// $lib/types/cropAspect.ts
//
// The crop tool's aspect lock. It only shapes the crop box while it is drawn;
// the crop itself stores no ratio.

import { fitCrop, turnedSize, type Crop } from "./crop";

export type AspectPresetId = "free" | "original" | "1:1" | "3:2" | "4:3" | "5:4" | "16:9";

export interface AspectPreset {
    id: AspectPresetId;
    label: string;
    /** Long side over short side. Absent for Free, and for Original, which takes the image's. */
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
];

export interface AspectLock {
    preset: AspectPresetId;
    /** Taller than wide on screen. */
    portrait: boolean;
}

export const FREE_ASPECT: AspectLock = { preset: "free", portrait: false };

// Ratios this close are taken as the same.
const ASPECT_TOLERANCE = 1e-3;

/** The lock's width over height, in pixels, or null when free. */
export function aspectRatio(lock: AspectLock, width: number, height: number): number | null {
    if (lock.preset === "free") return null;
    const long =
        lock.preset === "original"
            ? Math.max(width, height) / Math.min(width, height)
            : (ASPECT_PRESETS.find((p) => p.id === lock.preset)?.ratio ?? 1);
    return lock.portrait ? 1 / long : long;
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
