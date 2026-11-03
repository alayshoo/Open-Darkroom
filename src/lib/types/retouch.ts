// $lib/types/retouch.ts
//
// Heal and clone strokes, stored resolution-independently on the source image
// as it was decoded: before the crop turns, flips or rotates it. Mirrored in
// src-tauri/src/retouch.rs, which applies the same geometry on export.

export type RetouchMode = "heal" | "clone";

export interface Retouch {
    mode: RetouchMode;
    /** The stroke's centre line, as fractions of the source's width and height. One point is a spot. */
    points: [number, number][];
    /** Brush radius, as a fraction of the source's long edge. */
    radius: number;
    /** How much of the radius the edge fades over, 0 to 1. */
    feather: number;
    /** 0 to 1. */
    opacity: number;
    /** Where the pixels are taken from, relative to the stroke, as fractions of width and height. */
    offset: [number, number];
}

const isFiniteNumber = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);

function readPair(v: unknown): [number, number] | null {
    return Array.isArray(v) && v.length === 2 && isFiniteNumber(v[0]) && isFiniteNumber(v[1]) ? [v[0], v[1]] : null;
}

/** A stroke read from anywhere outside the app, or null when it cannot be applied. */
export function readRetouch(value: unknown): Retouch | null {
    if (!value || typeof value !== "object") return null;
    const v = value as Record<string, unknown>;
    if (v.mode !== "heal" && v.mode !== "clone") return null;
    if (!Array.isArray(v.points) || v.points.length === 0) return null;
    const points: [number, number][] = [];
    for (const p of v.points) {
        const pair = readPair(p);
        if (!pair) return null;
        points.push(pair);
    }
    const offset = readPair(v.offset);
    if (!offset || !isFiniteNumber(v.radius) || !(v.radius > 0)) return null;
    return {
        mode: v.mode,
        points,
        radius: v.radius,
        feather: isFiniteNumber(v.feather) ? clamp01(v.feather) : 0,
        opacity: isFiniteNumber(v.opacity) ? clamp01(v.opacity) : 1,
        offset,
    };
}

export function cloneRetouch(r: Retouch): Retouch {
    return {
        ...r,
        points: r.points.map(([x, y]) => [x, y]),
        offset: [r.offset[0], r.offset[1]],
    };
}

export function retouchEqual(a: Retouch, b: Retouch): boolean {
    return (
        a.mode === b.mode &&
        a.radius === b.radius &&
        a.feather === b.feather &&
        a.opacity === b.opacity &&
        a.offset[0] === b.offset[0] &&
        a.offset[1] === b.offset[1] &&
        a.points.length === b.points.length &&
        a.points.every((p, i) => p[0] === b.points[i][0] && p[1] === b.points[i][1])
    );
}

// ── Geometry in texels ────────────────────────────────────────────────────────
//
// Everything below must match retouch.rs, so the preview and the export cut a
// stroke into the same passes.

/** Output tiles a stroke is applied in, in texels on each side. */
export const RETOUCH_TILE = 512;

/** Most taps on either side of a blur; wider blurs stride across the source. */
export const MAX_BLUR_TAPS = 64;

/** [x0, y0, x1, y1] in texels, half open. */
export type TexelBox = [number, number, number, number];

/** A stroke laid on an image `width` × `height` texels. */
export interface PlacedRetouch {
    mode: RetouchMode;
    points: [number, number][];
    radius: number;
    feather: number;
    opacity: number;
    /** Whole texels, so the source's grain is copied rather than resampled. */
    offset: [number, number];
}

export function placeRetouch(r: Retouch, width: number, height: number): PlacedRetouch {
    return {
        mode: r.mode,
        points: r.points.map(([x, y]) => [x * width, y * height]),
        radius: r.radius * Math.max(width, height),
        feather: r.feather,
        opacity: r.opacity,
        offset: [Math.round(r.offset[0] * width), Math.round(r.offset[1] * height)],
    };
}

/** The blur that spreads the tone match in from around the stroke. */
export function blurSigma(radius: number): number {
    return Math.max(1, radius * 0.5);
}

export interface BlurSpan {
    sigma: number;
    /** Texels the kernel reaches on either side. */
    reach: number;
    /** Texels between taps. */
    step: number;
    /** Taps on either side of the centre. */
    taps: number;
}

export function blurSpan(radius: number): BlurSpan {
    const sigma = blurSigma(radius);
    const reach = Math.ceil(3 * sigma);
    const step = Math.ceil(reach / MAX_BLUR_TAPS);
    return { sigma, reach, step, taps: Math.ceil(reach / step) };
}

function clampBox([x0, y0, x1, y1]: TexelBox, width: number, height: number): TexelBox | null {
    const box: TexelBox = [Math.max(0, x0), Math.max(0, y0), Math.min(width, x1), Math.min(height, y1)];
    return box[2] > box[0] && box[3] > box[1] ? box : null;
}

/** The texels a stroke can change. */
export function retouchBounds(r: PlacedRetouch, width: number, height: number): TexelBox | null {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const [x, y] of r.points) {
        x0 = Math.min(x0, x);
        y0 = Math.min(y0, y);
        x1 = Math.max(x1, x);
        y1 = Math.max(y1, y);
    }
    const reach = r.radius + 1;
    return clampBox(
        [Math.floor(x0 - reach), Math.floor(y0 - reach), Math.ceil(x1 + reach), Math.ceil(y1 + reach)],
        width,
        height,
    );
}

/** The stroke as segments [ax, ay, bx, by]; a spot is one segment of no length. */
export function strokeSegments(points: [number, number][]): number[][] {
    if (points.length === 1) return [[...points[0], ...points[0]]];
    const segments: number[][] = [];
    for (let i = 1; i < points.length; i++) segments.push([...points[i - 1], ...points[i]]);
    return segments;
}

// Whether the segment crosses the box grown by `by`, clipped Liang–Barsky style.
function segmentNearBox([ax, ay, bx, by]: number[], [x0, y0, x1, y1]: TexelBox, grow: number): boolean {
    const dx = bx - ax;
    const dy = by - ay;
    let t0 = 0;
    let t1 = 1;
    const edges: [number, number][] = [
        [-dx, ax - (x0 - grow)],
        [dx, x1 + grow - ax],
        [-dy, ay - (y0 - grow)],
        [dy, y1 + grow - ay],
    ];
    for (const [p, q] of edges) {
        if (p === 0) {
            if (q < 0) return false;
        } else {
            const t = q / p;
            if (p < 0) t0 = Math.max(t0, t);
            else t1 = Math.min(t1, t);
            if (t0 > t1) return false;
        }
    }
    return true;
}

/** One pass of a stroke: what it writes, what its blur reads, and the segments that reach it. */
export interface RetouchTile {
    target: TexelBox;
    work: TexelBox;
    segments: number[][];
}

export function retouchTiles(r: PlacedRetouch, width: number, height: number): RetouchTile[] {
    const bounds = retouchBounds(r, width, height);
    if (!bounds) return [];
    const { reach } = blurSpan(r.radius);
    const margin = r.mode === "heal" ? reach : 0;
    const segments = strokeSegments(r.points);
    const tiles: RetouchTile[] = [];
    for (let y = bounds[1]; y < bounds[3]; y += RETOUCH_TILE) {
        for (let x = bounds[0]; x < bounds[2]; x += RETOUCH_TILE) {
            const target: TexelBox = [x, y, Math.min(x + RETOUCH_TILE, bounds[2]), Math.min(y + RETOUCH_TILE, bounds[3])];
            const near = segments.filter((s) => segmentNearBox(s, target, r.radius + 1));
            if (near.length === 0) continue;
            const work = clampBox(
                [target[0] - margin, target[1] - margin, target[2] + margin, target[3] + margin],
                width,
                height,
            )!;
            // The blur reads the stroke's mask across the whole work box.
            const reaching = margin > 0 ? segments.filter((s) => segmentNearBox(s, work, r.radius + 1)) : near;
            tiles.push({ target, work, segments: reaching });
        }
    }
    return tiles;
}

/**
 * The texels a tile reads from the image: its work box where the tone match is
 * taken, and the same box moved by the offset where the pixels come from.
 */
export function tileReads(tile: RetouchTile, offset: [number, number], width: number, height: number): TexelBox | null {
    const [x0, y0, x1, y1] = tile.work;
    const [dx, dy] = offset;
    return clampBox(
        [Math.min(x0, x0 + dx), Math.min(y0, y0 + dy), Math.max(x1, x1 + dx), Math.max(y1, y1 + dy)],
        width,
        height,
    );
}

/** Size of the `RetouchPass` uniform in retouch.wgsl. */
export const RETOUCH_UNIFORM_SIZE = 80;

/** The `RetouchPass` uniform for one tile. */
export function retouchUniform(
    r: PlacedRetouch,
    tile: RetouchTile,
    imageOrigin: [number, number],
    width: number,
    height: number,
): ArrayBuffer {
    const span = blurSpan(r.radius);
    const buffer = new ArrayBuffer(RETOUCH_UNIFORM_SIZE);
    const f = new Float32Array(buffer);
    const u = new Uint32Array(buffer);
    f.set([tile.target[0], tile.target[1]], 0);
    f.set([tile.work[0], tile.work[1]], 2);
    f.set([tile.work[2] - tile.work[0], tile.work[3] - tile.work[1]], 4);
    f.set(imageOrigin, 6);
    f.set([width, height], 8);
    f.set(r.offset, 10);
    f.set([r.radius, r.feather, r.opacity, span.sigma, span.step], 12);
    u.set([span.taps, tile.segments.length, r.mode === "heal" ? 1 : 0], 17);
    return buffer;
}

export function segmentsBuffer(tile: RetouchTile): Float32Array {
    return new Float32Array(tile.segments.flat());
}
