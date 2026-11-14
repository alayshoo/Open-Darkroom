// $lib/types/imageEdit.ts
//
// Everything done to one image, as one document: what the editor holds while
// the image is open, and the unit a library stores against it. Versioned, and
// read leniently, so a document written by another build still opens.

import { type Sliders, defaultSlidersRGB } from "./imgParameters";
import { CURVE_CHANNELS, type Curves, cloneCurves, defaultCurves } from "./curves";
import { type Crop, FULL_FRAME, isValidCrop } from "./crop";
import { type Retouch, cloneRetouch, readRetouch } from "./retouch";

export const IMAGE_EDIT_VERSION = 1;

export interface ImageEdit {
    version: number;
    /** The full-resolution size of the image the edit was made on. */
    source: { width: number; height: number };
    sliders: Sliders;
    /** False in B&W. */
    isRgb: boolean;
    /** The colour values set aside while in B&W, for the toggle back to colour. */
    heldColour: Partial<Sliders> | null;
    curves: Curves;
    crop: Crop;
    /** Heal and clone strokes, in the order they are applied. */
    retouch: Retouch[];
}

export function newImageEdit(width: number, height: number): ImageEdit {
    return {
        version: IMAGE_EDIT_VERSION,
        source: { width, height },
        sliders: { ...defaultSlidersRGB },
        isRgb: true,
        heldColour: null,
        curves: defaultCurves(),
        crop: { ...FULL_FRAME },
        retouch: [],
    };
}

export function cloneImageEdit(edit: ImageEdit): ImageEdit {
    return {
        version: edit.version,
        source: { ...edit.source },
        sliders: { ...edit.sliders },
        isRgb: edit.isRgb,
        heldColour: edit.heldColour ? { ...edit.heldColour } : null,
        curves: cloneCurves(edit.curves),
        crop: { ...edit.crop },
        retouch: edit.retouch.map(cloneRetouch),
    };
}

/** Whether an edit was made on an image of this size, so its strokes land where they were drawn. */
export function editFitsSource(edit: ImageEdit, width: number, height: number): boolean {
    return edit.source.width === width && edit.source.height === height;
}

export function serializeImageEdit(edit: ImageEdit): string {
    return JSON.stringify({ ...cloneImageEdit(edit), version: IMAGE_EDIT_VERSION });
}

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);

// Keys this build does not know are dropped, and those it expects but cannot
// read keep their defaults.
export function readSliderValues(value: unknown): Partial<Sliders> {
    const out: Partial<Sliders> = {};
    if (!isRecord(value)) return out;
    for (const key of Object.keys(defaultSlidersRGB) as (keyof Sliders)[]) {
        const v = value[key];
        if (typeof v === typeof defaultSlidersRGB[key] && (typeof v !== "number" || Number.isFinite(v))) {
            (out as Record<string, unknown>)[key] = v;
        }
    }
    return out;
}

export function readCurveValues(value: unknown): Curves {
    const curves = defaultCurves();
    if (!isRecord(value)) return curves;
    for (const channel of CURVE_CHANNELS) {
        const points = value[channel];
        if (!Array.isArray(points) || points.length < 2) continue;
        const read = points.map((p) => (isRecord(p) ? { x: p.x, y: p.y } : null));
        const valid = read.every(
            (p, i) =>
                p &&
                typeof p.x === "number" &&
                typeof p.y === "number" &&
                p.y >= 0 &&
                p.y <= 1 &&
                (i === 0 ? p.x === 0 : p.x > (read[i - 1]!.x as number)),
        );
        if (valid && read[read.length - 1]!.x === 1) curves[channel] = read as Curves[typeof channel];
    }
    return curves;
}

export function readCrop(value: unknown): Crop {
    if (!isRecord(value)) return { ...FULL_FRAME };
    const crop = { ...FULL_FRAME, ...value } as Crop;
    const turns = crop.quarterTurns;
    if (!isValidCrop(crop) || !Number.isInteger(turns) || turns < 0 || turns > 3) return { ...FULL_FRAME };
    return {
        cx: crop.cx,
        cy: crop.cy,
        w: crop.w,
        h: crop.h,
        angle: crop.angle,
        quarterTurns: turns,
        flipX: crop.flipX === true,
        flipY: crop.flipY === true,
    };
}

/** An edit read back from `text`, or null when it is not an edit document at all. */
export function parseImageEdit(text: string): ImageEdit | null {
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch {
        return null;
    }
    if (!isRecord(value) || typeof value.version !== "number" || !isRecord(value.source)) return null;
    const { width, height } = value.source;
    if (typeof width !== "number" || typeof height !== "number" || !(width > 0 && height > 0)) return null;

    const edit = newImageEdit(width, height);
    edit.sliders = { ...edit.sliders, ...readSliderValues(value.sliders) };
    edit.isRgb = value.isRgb !== false;
    edit.heldColour = value.heldColour ? readSliderValues(value.heldColour) : null;
    edit.curves = readCurveValues(value.curves);
    edit.crop = readCrop(value.crop);
    edit.retouch = Array.isArray(value.retouch)
        ? value.retouch.map(readRetouch).filter((r): r is Retouch => r !== null)
        : [];
    return edit;
}
