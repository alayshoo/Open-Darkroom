// src/lib/utils/viewport.ts
//
// Where the image sits on the preview canvas, and which part of the image the
// render chain has to hold to show it. Canvas coordinates are device pixels
// from the canvas's top-left corner; image coordinates are full-resolution
// pixels. The chain works in image pixels scaled by its render scale, and holds
// one box of them at a time.

import {
    cropView,
    rotatedBounds,
    rotationCanvas,
    uprightFrame,
    type Affine,
    type Crop,
    type CropView,
} from "$lib/types/crop";

export type Point = [number, number];
/** [x0, y0, x1, y1] */
export type Box = [number, number, number, number];

/** Most canvas pixels one image pixel may be enlarged to. */
export const MAX_ZOOM = 8;

/** Render pixels kept on every side of what is visible, so a short pan re-renders nothing. */
export const PAN_MARGIN = 192;

// ── Affine maps and boxes ─────────────────────────────────────────────────────

export function apply(a: Affine, [x, y]: Point): Point {
    return [
        a.origin[0] + x * a.axisX[0] + y * a.axisY[0],
        a.origin[1] + x * a.axisX[1] + y * a.axisY[1],
    ];
}

/** `outer` after `inner`. */
export function compose(outer: Affine, inner: Affine): Affine {
    const linear = ([x, y]: Point): Point => [
        x * outer.axisX[0] + y * outer.axisY[0],
        x * outer.axisX[1] + y * outer.axisY[1],
    ];
    return { origin: apply(outer, inner.origin), axisX: linear(inner.axisX), axisY: linear(inner.axisY) };
}

export function invert(a: Affine): Affine {
    const [[a0, a1], [b0, b1]] = [a.axisX, a.axisY];
    const det = a0 * b1 - b0 * a1;
    const axisX: Point = [b1 / det, -a1 / det];
    const axisY: Point = [-b0 / det, a0 / det];
    const [ox, oy] = a.origin;
    return {
        origin: [-(ox * axisX[0] + oy * axisY[0]), -(ox * axisX[1] + oy * axisY[1])],
        axisX,
        axisY,
    };
}

/** (x, y) ↦ (x · sx + tx, y · sy + ty) */
export function scaleBy(sx: number, sy: number, tx = 0, ty = 0): Affine {
    return { origin: [tx, ty], axisX: [sx, 0], axisY: [0, sy] };
}

export function corners([x0, y0, x1, y1]: Box): Point[] {
    return [[x0, y0], [x1, y0], [x0, y1], [x1, y1]];
}

export function bounds(points: Point[]): Box {
    const xs = points.map((p) => p[0]);
    const ys = points.map((p) => p[1]);
    return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

/** `box` carried through `a`, as the box around where its corners land. */
export function mapBox(a: Affine, box: Box): Box {
    return bounds(corners(box).map((p) => apply(a, p)));
}

export function intersect(a: Box, b: Box): Box | null {
    const box: Box = [Math.max(a[0], b[0]), Math.max(a[1], b[1]), Math.min(a[2], b[2]), Math.min(a[3], b[3])];
    return box[0] < box[2] && box[1] < box[3] ? box : null;
}

export function contains(outer: Box, inner: Box): boolean {
    return outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3];
}

export function grow([x0, y0, x1, y1]: Box, by: number): Box {
    return [x0 - by, y0 - by, x1 + by, y1 + by];
}

// ── Placing the image ─────────────────────────────────────────────────────────

/**
 * A crop's output laid on the canvas at `scale` canvas pixels per output pixel,
 * with output point `center` over canvas point `anchor`. The output is at full
 * resolution, so `scale` is also canvas pixels per image pixel.
 */
export interface Placement {
    view: CropView;
    scale: number;
    center: Point;
    anchor: Point;
}

/** The box the image is fitted into, in canvas pixels. */
export interface Slot {
    center: Point;
    width: number;
    height: number;
}

export function canvasToOutput(p: Placement): Affine {
    const s = 1 / p.scale;
    return scaleBy(s, s, p.center[0] - p.anchor[0] * s, p.center[1] - p.anchor[1] * s);
}

export function canvasToImage(p: Placement): Affine {
    return compose(p.view, canvasToOutput(p));
}

/** The largest scale that fits `width` × `height` into the slot, never past 1:1. */
export function fitScale(slot: Slot, width: number, height: number): number {
    if (!(slot.width > 0 && slot.height > 0)) return 1;
    return Math.min(1, slot.width / width, slot.height / height);
}

export function clampCenter(center: Point, view: CropView): Point {
    return [
        Math.min(Math.max(center[0], 0), view.width),
        Math.min(Math.max(center[1], 0), view.height),
    ];
}

/** The centre that keeps the output point under `cursor` where it is at a new scale. */
export function zoomAbout(p: Placement, scale: number, cursor: Point): Point {
    const [qx, qy] = apply(canvasToOutput(p), cursor);
    return [qx - (cursor[0] - p.anchor[0]) / scale, qy - (cursor[1] - p.anchor[1]) / scale];
}

/** What the canvas shows. */
export interface Layout {
    placement: Placement;
    // Canvas pixels outside this box are left clear, or null to draw all of
    // the image that lands on the canvas.
    clip: Box | null;
    // Canvas pixels outside this box are shaded.
    keep?: Box;
    // The scale that fits the image to the slot, which a zoom can never go below.
    fit: number;
    // The frame the crop tool's overlay is drawn over, in canvas pixels.
    frame: { width: number; height: number };
}

/**
 * The crop on its own, fitted to the slot, or at `zoom` canvas pixels per image
 * pixel with output point `center` over the middle of the slot.
 */
export function cropLayout(
    crop: Crop,
    fullWidth: number,
    fullHeight: number,
    slot: Slot,
    zoom: number | null,
    center: Point | null,
): Layout {
    const view = cropView(crop, fullWidth, fullHeight);
    const fit = fitScale(slot, view.width, view.height);
    const placement: Placement = {
        view,
        scale: zoom ?? fit,
        center: zoom !== null && center ? clampCenter(center, view) : [view.width / 2, view.height / 2],
        anchor: slot.center,
    };
    return {
        placement,
        clip: mapBox(invert(canvasToOutput(placement)), [0, 0, view.width, view.height]),
        fit,
        frame: { width: view.width * fit, height: view.height * fit },
    };
}

/**
 * All of the turned and rotated image the crop is cut from, laid out on its
 * upright frame, with everything outside the crop box shaded.
 */
export function wholeLayout(crop: Crop, fullWidth: number, fullHeight: number, slot: Slot): Layout {
    const view = cropView(rotationCanvas(crop, fullWidth, fullHeight), fullWidth, fullHeight);
    const reach = cropView(rotatedBounds(crop, fullWidth, fullHeight), fullWidth, fullHeight);
    const upright = cropView(uprightFrame(crop), fullWidth, fullHeight);
    const scale = fitScale(slot, reach.width, reach.height);
    const placement: Placement = {
        view,
        scale,
        center: [view.width / 2, view.height / 2],
        anchor: slot.center,
    };

    const fw = upright.width * scale;
    const fh = upright.height * scale;
    const left = slot.center[0] - fw / 2;
    const top = slot.center[1] - fh / 2;
    return {
        placement,
        clip: null,
        keep: [
            left + (crop.cx - crop.w / 2) * fw,
            top + (crop.cy - crop.h / 2) * fh,
            left + (crop.cx + crop.w / 2) * fw,
            top + (crop.cy + crop.h / 2) * fh,
        ],
        fit: scale,
        frame: { width: fw, height: fh },
    };
}

// ── What the renderer is asked to show ────────────────────────────────────────

export interface ViewFrame {
    canvasWidth: number;
    canvasHeight: number;
    toImage: Affine;
    // Canvas pixels per image pixel.
    scale: number;
    clip: Box | null;
    keep?: Box;
    fullWidth: number;
    fullHeight: number;
}

export function viewFrame(layout: Layout, canvasWidth: number, canvasHeight: number, fullWidth: number, fullHeight: number): ViewFrame {
    return {
        canvasWidth,
        canvasHeight,
        toImage: canvasToImage(layout.placement),
        scale: layout.placement.scale,
        clip: layout.clip,
        keep: layout.keep,
        fullWidth,
        fullHeight,
    };
}

/** The part of the canvas the image is drawn on, and the part of the image it shows. */
export interface Visible {
    canvas: Box;
    image: Box;
}

export function visibleRegion(frame: ViewFrame): Visible | null {
    const image: Box = [0, 0, frame.fullWidth, frame.fullHeight];
    let canvas = intersect(mapBox(invert(frame.toImage), image), [0, 0, frame.canvasWidth, frame.canvasHeight]);
    if (canvas && frame.clip) canvas = intersect(canvas, frame.clip);
    if (!canvas) return null;
    const shown = intersect(mapBox(frame.toImage, canvas), image);
    return shown ? { canvas, image: shown } : null;
}

// ── The chain's region ────────────────────────────────────────────────────────

/**
 * What the chain's textures hold: image pixel (x, y) is chain texel
 * (x · scale − box[0], y · scale − box[1]), and `apron` texels on every side of
 * the box only feed the blurs.
 */
export interface ChainRegion {
    scale: number;
    box: Box;
    apron: number;
}

/** The chain texels drawing `image` reads, at render scale `scale`. */
export function neededBox(image: Box, scale: number): Box {
    return [
        Math.floor(image[0] * scale) - 1,
        Math.floor(image[1] * scale) - 1,
        Math.ceil(image[2] * scale) + 1,
        Math.ceil(image[3] * scale) + 1,
    ];
}

/** The part of a region whose pixels are fully rendered. */
export function validBox(region: ChainRegion): Box {
    return grow(region.box, -region.apron);
}

/**
 * The region to render to cover `needed`: `margin` more on every side as far
 * as the image goes, then the apron, never wider than `maxSize`.
 */
export function planRegion(
    needed: Box,
    scale: number,
    apron: number,
    margin: number,
    fullWidth: number,
    fullHeight: number,
    maxSize: number,
): ChainRegion {
    const image: Box = [-1, -1, Math.ceil(fullWidth * scale) + 1, Math.ceil(fullHeight * scale) + 1];
    const kept = intersect(grow(needed, margin), image) ?? needed;
    const box = grow(kept, apron);
    box[2] = Math.min(box[2], box[0] + maxSize);
    box[3] = Math.min(box[3], box[1] + maxSize);
    return { scale, box, apron };
}

/** Whether a region rendered at another scale can stand in while a zoom is under way. */
export function standsIn(region: ChainRegion, scale: number): boolean {
    const ratio = scale / region.scale;
    return ratio >= 0.5 && ratio <= 2;
}

/** Canvas pixel → chain texel. */
export function canvasToChain(frame: ViewFrame, region: ChainRegion): Affine {
    const s = region.scale;
    return snapToTexels(compose(scaleBy(s, s, -region.box[0], -region.box[1]), frame.toImage));
}

/** Chain texel → texel of a source `sourceWidth` × `sourceHeight` covering the whole image. */
export function chainToSource(
    region: ChainRegion,
    sourceWidth: number,
    sourceHeight: number,
    fullWidth: number,
    fullHeight: number,
): Affine {
    const kx = sourceWidth / fullWidth / region.scale;
    const ky = sourceHeight / fullHeight / region.scale;
    return snapToTexels(scaleBy(kx, ky, region.box[0] * kx, region.box[1] * ky));
}

/** The image's own extent in chain texels. */
export function imageInChain(region: ChainRegion, fullWidth: number, fullHeight: number): Box {
    const s = region.scale;
    return [-region.box[0], -region.box[1], fullWidth * s - region.box[0], fullHeight * s - region.box[1]];
}

// A map that steps one texel per pixel along the target's own axes is moved to
// land on texel centres, so 1:1 shows the pixels themselves rather than blends
// of neighbours.
export function snapToTexels(a: Affine): Affine {
    const unit = [...a.axisX, ...a.axisY].every((v) => Math.abs(v) < 1e-9 || Math.abs(Math.abs(v) - 1) < 1e-9);
    if (!unit) return a;
    const round = (v: number) => (Math.abs(v) < 1e-9 ? 0 : Math.sign(v));
    return {
        origin: [Math.round(a.origin[0]), Math.round(a.origin[1])],
        axisX: [round(a.axisX[0]), round(a.axisX[1])],
        axisY: [round(a.axisY[0]), round(a.axisY[1])],
    };
}
