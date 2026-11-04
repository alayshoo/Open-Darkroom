// $lib/utils/retouchSource.ts
//
// Where a stroke could take its pixels from when none was picked: offsets
// that clear the stroke, and the ring of texels around it they are judged on.
// All in texels of the image being searched.

import type { PlacedRetouch } from "$lib/types/retouch";

type Point = [number, number];

/** Most texels the ring is judged on. */
const MAX_SAMPLES = 1024;

/** The stroke resampled every `spacing` texels, ends included. */
export function resampleStroke(points: Point[], spacing: number): Point[] {
    if (points.length === 1) return [points[0]];
    const out: Point[] = [points[0]];
    let carried = 0;
    for (let i = 1; i < points.length; i++) {
        const [ax, ay] = points[i - 1];
        const [bx, by] = points[i];
        const length = Math.hypot(bx - ax, by - ay);
        let at = spacing - carried;
        while (at <= length) {
            const t = at / length;
            out.push([ax + (bx - ax) * t, ay + (by - ay) * t]);
            at += spacing;
        }
        carried = length - (at - spacing);
    }
    const last = points[points.length - 1];
    const tail = out[out.length - 1];
    if (tail[0] !== last[0] || tail[1] !== last[1]) out.push(last);
    return out;
}

function thin<T>(items: T[], max: number): T[] {
    if (items.length <= max) return items;
    const step = items.length / max;
    return Array.from({ length: max }, (_, i) => items[Math.floor(i * step)]);
}

const inside = ([x, y]: Point, width: number, height: number) => x >= 0 && y >= 0 && x < width && y < height;

/** Texels just clear of the stroke on every side of it. */
export function ringSamples(r: PlacedRetouch, width: number, height: number): Point[] {
    const near = r.radius * 1.4 + 1;
    const far = r.radius * 2 + 2;
    const centres = resampleStroke(r.points, Math.max(1, r.radius * 0.5));
    const samples: Point[] = [];
    const rings = Math.max(12, Math.ceil(r.radius));
    for (const [cx, cy] of centres) {
        for (let i = 0; i < rings; i++) {
            const a = (2 * Math.PI * i) / rings;
            for (const d of [near, far]) samples.push([cx + Math.cos(a) * d, cy + Math.sin(a) * d]);
        }
    }
    const dense = resampleStroke(r.points, Math.max(0.5, r.radius * 0.25));
    const clear = samples.filter(
        (p) => inside(p, width, height) && nearest(p, dense) > r.radius + 1,
    );
    return thin(clear, MAX_SAMPLES).map(([x, y]) => [Math.floor(x) + 0.5, Math.floor(y) + 0.5]);
}

function nearest([x, y]: Point, points: Point[]): number {
    let d = Infinity;
    for (const [px, py] of points) d = Math.min(d, Math.hypot(px - x, py - y));
    return d;
}

/** The closest the stroke comes to itself moved by `offset`. */
function clearance(points: Point[], [dx, dy]: Point): number {
    let d = Infinity;
    for (const [x, y] of points) d = Math.min(d, nearest([x + dx, y + dy], points));
    return d;
}

// Whole-texel offsets that move the stroke clear of itself and keep it on the
// image, nearest rings first; wider rings only when the nearer leave nothing.
export function sourceCandidates(r: PlacedRetouch, width: number, height: number): Point[] {
    const coarse = thin(resampleStroke(r.points, Math.max(1, r.radius * 0.5)), 96);
    const xs = r.points.map((p) => p[0]);
    const ys = r.points.map((p) => p[1]);
    const box = [Math.min(...xs) - r.radius, Math.min(...ys) - r.radius, Math.max(...xs) + r.radius, Math.max(...ys) + r.radius];
    const fits = ([dx, dy]: Point) =>
        box[0] + dx >= 0 && box[1] + dy >= 0 && box[2] + dx <= width && box[3] + dy <= height;

    const gap = 2 * r.radius + 2;
    const directions = 24;
    for (const ring of [[2.5, 3.5, 5], [7, 10], [14, 20, 28]]) {
        const found: Point[] = [];
        for (const k of ring) {
            const distance = k * r.radius + 3;
            for (let i = 0; i < directions; i++) {
                const a = (2 * Math.PI * (i + (k % 2) * 0.5)) / directions;
                const offset: Point = [Math.round(Math.cos(a) * distance), Math.round(Math.sin(a) * distance)];
                if (fits(offset) && clearance(coarse, offset) >= gap) found.push(offset);
            }
        }
        if (found.length > 0) return found;
    }
    return [];
}

/** The best of `scores`, which are lower for a better match, preferring nearer offsets. */
export function bestCandidate(candidates: Point[], scores: ArrayLike<number>, radius: number): Point | null {
    let best: Point | null = null;
    let bestScore = Infinity;
    candidates.forEach((c, i) => {
        const score = scores[i] * (1 + (0.05 * Math.hypot(c[0], c[1])) / (radius + 1));
        if (score < bestScore) {
            bestScore = score;
            best = c;
        }
    });
    return bestScore < 1e29 ? best : null;
}
