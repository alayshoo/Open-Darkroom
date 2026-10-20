// $lib/types/crop.ts
//
// The crop, stored resolution-independently so the downscaled preview and the
// full-resolution export read the same edit. Mirrored in src-tauri/src/crop.rs.

export interface Crop {
    /** Centre, as fractions of the full image's width and height. */
    cx: number;
    cy: number;
    /** Extent, as fractions of the full image's width and height. */
    w: number;
    h: number;
    /** Radians, clockwise on screen. Nothing sets it yet. */
    angle: number;
}

export const FULL_FRAME: Crop = { cx: 0.5, cy: 0.5, w: 1, h: 1, angle: 0 };

export function isFullFrame(c: Crop): boolean {
    return c.cx === 0.5 && c.cy === 0.5 && c.w === 1 && c.h === 1 && c.angle === 0;
}

export function cropsEqual(a: Crop, b: Crop): boolean {
    return a.cx === b.cx && a.cy === b.cy && a.w === b.w && a.h === b.h && a.angle === b.angle;
}

/**
 * Output pixels mapped back into a source of a given size: the source texel
 * coordinate of output point (x, y) is `origin + x * axisX + y * axisY`.
 */
export interface CropView {
    width: number;
    height: number;
    origin: [number, number];
    axisX: [number, number];
    axisY: [number, number];
}

/**
 * An unrotated crop is snapped to whole source pixels, so every output pixel
 * lands on a texel centre and the encode stage reads it back untouched.
 */
export function cropView(crop: Crop, srcWidth: number, srcHeight: number): CropView {
    if (crop.angle === 0) {
        const snap = (centre: number, extent: number, size: number) => {
            const lo = Math.min(Math.max(Math.round((centre - extent / 2) * size), 0), size - 1);
            const hi = Math.min(Math.max(Math.round((centre + extent / 2) * size), lo + 1), size);
            return [lo, hi - lo];
        };
        const [x0, width] = snap(crop.cx, crop.w, srcWidth);
        const [y0, height] = snap(crop.cy, crop.h, srcHeight);
        return { width, height, origin: [x0, y0], axisX: [1, 0], axisY: [0, 1] };
    }

    const width = Math.max(1, Math.round(crop.w * srcWidth));
    const height = Math.max(1, Math.round(crop.h * srcHeight));
    const cos = Math.cos(crop.angle);
    const sin = Math.sin(crop.angle);
    const centreX = crop.cx * srcWidth;
    const centreY = crop.cy * srcHeight;
    return {
        width,
        height,
        origin: [
            centreX - (width / 2) * cos + (height / 2) * sin,
            centreY - (width / 2) * sin - (height / 2) * cos,
        ],
        axisX: [cos, sin],
        axisY: [-sin, cos],
    };
}

/** The `CropView` uniform, as chunks/cropView.wgsl declares it. */
export const CROP_VIEW_BUFFER_SIZE = 32;

export function cropViewToFloats(view: CropView): Float32Array {
    return new Float32Array([
        ...view.origin,
        ...view.axisX,
        ...view.axisY,
        view.width,
        view.height,
    ]);
}
