// src/lib/utils/viewport.ts
//
// Where the image sits on the preview canvas, and what one frame of the render
// chain needs to cover to show it. Canvas coordinates are device pixels from
// the canvas's top-left corner; image coordinates are full-resolution pixels.

import {
    cropView,
    rotatedBounds,
    rotationCanvas,
    uprightFrame,
    type Affine,
    type Crop,
    type CropView,
} from "$lib/types/crop";
import type { RenderFrame } from "$lib/types/gpuTypes";

export type Point = [number, number];
/** [x0, y0, x1, y1] */
export type Box = [number, number, number, number];

/** Most canvas pixels one image pixel may be enlarged to. */
export const MAX_ZOOM = 8;

/** Furthest the sharpness blurs reach, in render pixels. Must match MAX_RADIUS in calcSharpTexture.wgsl. */
export const SHARP_REACH = 180;

// ── Affine maps ───────────────────────────────────────────────────────────────

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

export function bounds(points: Point[]): Box {
    const xs = points.map((p) => p[0]);
    const ys = points.map((p) => p[1]);
    return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
}

export function intersect(a: Box, b: Box): Box | null {
    const box: Box = [Math.max(a[0], b[0]), Math.max(a[1], b[1]), Math.min(a[2], b[2]), Math.min(a[3], b[3])];
    return box[0] < box[2] && box[1] < box[3] ? box : null;
}

export function contains(outer: Box, inner: Box): boolean {
    return outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3];
}

function corners([x0, y0, x1, y1]: Box): Point[] {
    return [[x0, y0], [x1, y0], [x0, y1], [x1, y1]];
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

/** What the canvas shows, and where nothing is drawn. */
export interface Layout {
    placement: Placement;
    // Canvas pixel → a square outside of which the canvas is left clear.
    clip: Affine;
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
        clip: compose(scaleBy(1 / view.width, 1 / view.height), canvasToOutput(placement)),
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
        clip: compose(scaleBy(1 / fullWidth, 1 / fullHeight), canvasToImage(placement)),
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

// ── Planning a frame ──────────────────────────────────────────────────────────

/** The part of the canvas the image covers, and the part of the image it shows. */
export interface Visible {
    canvas: Box;
    image: Box;
}

export function visibleRegion(
    layout: Layout,
    canvasWidth: number,
    canvasHeight: number,
    fullWidth: number,
    fullHeight: number,
): Visible | null {
    const drawn = bounds(corners([0, 0, 1, 1]).map((p) => apply(invert(layout.clip), p)));
    const canvas = intersect(drawn, [0, 0, canvasWidth, canvasHeight]);
    if (!canvas) return null;
    const toImage = canvasToImage(layout.placement);
    const image = intersect(bounds(corners(canvas).map((p) => apply(toImage, p))), [0, 0, fullWidth, fullHeight]);
    return image ? { canvas, image } : null;
}

/**
 * The index of the coarsest level with at least one texel per canvas pixel at
 * this scale, or level 0 at 1:1 and past it.
 */
export function levelFor(scale: number, levels: { width: number }[]): number {
    const needed = Math.min(scale, 1) * levels[0].width;
    let level = 0;
    while (level + 1 < levels.length && levels[level + 1].width >= needed * (1 - 1e-9)) level++;
    return level;
}

/**
 * A source texture holding part of one level: image pixel (x, y) is at texel
 * (x · kx − offset[0], y · ky − offset[1]).
 */
export interface SourceMapping {
    kx: number;
    ky: number;
    offset: Point;
}

/** How far past what is visible the chain has to render for the blurs, in render pixels. */
export function apronFor(density: number, scale: number, sharpening: boolean): number {
    if (!sharpening) return 2;
    return Math.ceil(Math.min(SHARP_REACH, SHARP_REACH * scale * density)) + 2;
}

export function planFrame(
    layout: Layout,
    visible: Visible,
    source: SourceMapping,
    sharpening: boolean,
    canvasWidth: number,
    canvasHeight: number,
): RenderFrame {
    const { scale } = layout.placement;
    // Past 1:1 the chain stays at full resolution and the last stage enlarges.
    const density = scale > 1 ? 1 / scale : 1;
    const apron = apronFor(density, scale, sharpening);

    const [cx0, cy0, cx1, cy1] = visible.canvas;
    const x = Math.floor(cx0 * density) - apron;
    const y = Math.floor(cy0 * density) - apron;
    const width = Math.ceil(cx1 * density) + apron - x;
    const height = Math.ceil(cy1 * density) + apron - y;

    const renderToCanvas = scaleBy(1 / density, 1 / density, x / density, y / density);
    const toTexture = scaleBy(source.kx, source.ky, -source.offset[0], -source.offset[1]);
    const sample = compose(toTexture, compose(canvasToImage(layout.placement), renderToCanvas));

    return {
        viewport: { x, y, width, height },
        density,
        sample: snapToTexels(sample),
        clip: layout.clip,
        keep: layout.keep,
        canvasWidth,
        canvasHeight,
    };
}

// A map that steps one texel per render pixel along the source's own axes is
// moved to land on texel centres, so 1:1 shows the pixels themselves rather than
// blends of neighbours.
function snapToTexels(a: Affine): Affine {
    const unit = [...a.axisX, ...a.axisY].every((v) => Math.abs(v) < 1e-9 || Math.abs(Math.abs(v) - 1) < 1e-9);
    if (!unit) return a;
    const round = (v: number) => (Math.abs(v) < 1e-9 ? 0 : Math.sign(v));
    return {
        origin: [Math.round(a.origin[0]), Math.round(a.origin[1])],
        axisX: [round(a.axisX[0]), round(a.axisX[1])],
        axisY: [round(a.axisY[0]), round(a.axisY[1])],
    };
}
