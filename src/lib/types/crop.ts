// $lib/types/crop.ts
//
// The crop, stored resolution-independently so the downscaled preview and the
// full-resolution export read the same edit. Mirrored in src-tauri/src/crop.rs.
//
// The image is first turned by whole quarter turns, then rotated by `angle`
// about its centre. The crop box is upright in that rotated frame, which shares
// the turned image's centre and size, so its fractions are of the turned image.

export interface Crop {
    /** Centre, as fractions of the turned image's width and height. */
    cx: number;
    cy: number;
    /** Extent, as fractions of the turned image's width and height. */
    w: number;
    h: number;
    /** Radians the image is rotated clockwise on screen, within ±MAX_ANGLE. */
    angle: number;
    /** Clockwise quarter turns, 0–3, applied before `angle`. */
    quarterTurns: number;
}

export const FULL_FRAME: Crop = { cx: 0.5, cy: 0.5, w: 1, h: 1, angle: 0, quarterTurns: 0 };

export const MAX_ANGLE = Math.PI / 4;

export function isFullFrame(c: Crop): boolean {
    return cropsEqual(c, FULL_FRAME);
}

/** Anything malformed is read as the whole frame, as crop.rs does. */
export function isValidCrop(c: Crop): boolean {
    return [c.cx, c.cy, c.w, c.h, c.angle].every(Number.isFinite) && c.w > 0 && c.h > 0;
}

export function cropsEqual(a: Crop, b: Crop): boolean {
    return (
        a.cx === b.cx &&
        a.cy === b.cy &&
        a.w === b.w &&
        a.h === b.h &&
        a.angle === b.angle &&
        a.quarterTurns === b.quarterTurns
    );
}

/** The turned image before any rotation: the frame the crop tool lays out. */
export function uprightFrame(c: Crop): Crop {
    return { ...FULL_FRAME, quarterTurns: c.quarterTurns };
}

/** All of the turned and rotated image, corners included. */
export function rotatedBounds(c: Crop, width: number, height: number): Crop {
    const [w, h] = turnedSize(c, width, height);
    const cos = Math.abs(Math.cos(c.angle));
    const sin = Math.abs(Math.sin(c.angle));
    return {
        ...uprightFrame(c),
        angle: c.angle,
        w: (cos * w + sin * h) / w,
        h: (sin * w + cos * h) / h,
    };
}

/** The image's size once turned. */
export function turnedSize(c: Crop, width: number, height: number): [number, number] {
    return c.quarterTurns % 2 === 0 ? [width, height] : [height, width];
}

/** The same crop on the image turned a further quarter turn either way. */
export function turnCrop(c: Crop, clockwise: boolean): Crop {
    return {
        cx: clockwise ? 1 - c.cy : c.cy,
        cy: clockwise ? c.cx : 1 - c.cx,
        w: c.h,
        h: c.w,
        angle: c.angle,
        quarterTurns: (c.quarterTurns + (clockwise ? 1 : 3)) % 4,
    };
}

// Slack on the inside test, in pixels, so a crop fitted exactly to the edge
// still counts as inside.
const INSIDE_SLACK = 1e-6;

/**
 * Where a point of the rotated frame lands on the turned image, relative to
 * its centre, in pixels.
 */
function toTurned(x: number, y: number, size: [number, number], angle: number): [number, number] {
    const dx = x - size[0] / 2;
    const dy = y - size[1] / 2;
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    return [cos * dx + sin * dy, -sin * dx + cos * dy];
}

/** Whether every corner of the crop falls on the image. */
export function cropInside(c: Crop, width: number, height: number): boolean {
    const size = turnedSize(c, width, height);
    for (const sx of [-1, 1]) {
        for (const sy of [-1, 1]) {
            const [x, y] = toTurned(
                (c.cx + (sx * c.w) / 2) * size[0],
                (c.cy + (sy * c.h) / 2) * size[1],
                size,
                c.angle,
            );
            if (Math.abs(x) > size[0] / 2 + INSIDE_SLACK) return false;
            if (Math.abs(y) > size[1] / 2 + INSIDE_SLACK) return false;
        }
    }
    return true;
}

/**
 * The largest copy of the crop, about the same centre and at the same aspect,
 * that stays on the image. A centre that has itself left the image is pulled
 * back onto it first.
 */
export function fitCrop(c: Crop, width: number, height: number): Crop {
    if (cropInside(c, width, height)) return c;
    const size = turnedSize(c, width, height);
    const half = [size[0] / 2, size[1] / 2];
    const cos = Math.cos(c.angle);
    const sin = Math.sin(c.angle);

    let [dx, dy] = toTurned(c.cx * size[0], c.cy * size[1], size, c.angle);
    if (Math.abs(dx) >= half[0] || Math.abs(dy) >= half[1]) {
        dx = Math.max(-half[0], Math.min(half[0], dx)) * 0.999;
        dy = Math.max(-half[1], Math.min(half[1], dy)) * 0.999;
    }
    const cx = (half[0] + cos * dx - sin * dy) / size[0];
    const cy = (half[1] + sin * dx + cos * dy) / size[1];

    const a = (c.w * size[0]) / 2;
    const b = (c.h * size[1]) / 2;
    let scale = 1;
    for (const sx of [-1, 1]) {
        for (const sy of [-1, 1]) {
            const ex = cos * sx * a + sin * sy * b;
            const ey = -sin * sx * a + cos * sy * b;
            for (const [d, e, h] of [[dx, ex, half[0]], [dy, ey, half[1]]]) {
                if (e > 0) scale = Math.min(scale, (h - d) / e);
                else if (e < 0) scale = Math.min(scale, (h + d) / -e);
            }
        }
    }
    scale = Math.max(scale, 0);

    return { ...c, cx, cy, w: c.w * scale, h: c.h * scale };
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
    if (!isValidCrop(crop)) crop = FULL_FRAME;
    const size = turnedSize(crop, srcWidth, srcHeight);
    let x0: number, y0: number, width: number, height: number;

    if (crop.angle === 0) {
        const snap = (centre: number, extent: number, size: number) => {
            const lo = Math.min(Math.max(Math.round((centre - extent / 2) * size), 0), size - 1);
            const hi = Math.min(Math.max(Math.round((centre + extent / 2) * size), lo + 1), size);
            return [lo, hi - lo];
        };
        [x0, width] = snap(crop.cx, crop.w, size[0]);
        [y0, height] = snap(crop.cy, crop.h, size[1]);
    } else {
        width = Math.max(1, Math.round(crop.w * size[0]));
        height = Math.max(1, Math.round(crop.h * size[1]));
        x0 = crop.cx * size[0] - width / 2;
        y0 = crop.cy * size[1] - height / 2;
    }

    // Rotated frame → turned image → source.
    const toSource = (x: number, y: number): [number, number] => {
        const [tx, ty] = toTurned(x, y, size, crop.angle);
        const ox = tx + size[0] / 2;
        const oy = ty + size[1] / 2;
        switch (crop.quarterTurns % 4) {
            case 1: return [oy, srcHeight - ox];
            case 2: return [srcWidth - ox, srcHeight - oy];
            case 3: return [srcWidth - oy, ox];
            default: return [ox, oy];
        }
    };

    const origin = toSource(x0, y0);
    const right = toSource(x0 + 1, y0);
    const down = toSource(x0, y0 + 1);
    return {
        width,
        height,
        origin,
        axisX: [right[0] - origin[0], right[1] - origin[1]],
        axisY: [down[0] - origin[0], down[1] - origin[1]],
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
