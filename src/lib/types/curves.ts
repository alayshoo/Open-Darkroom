// $lib/types/curves.ts

export const CURVE_CHANNELS = ["master", "red", "green", "blue"] as const;
export type CurveChannel = (typeof CURVE_CHANNELS)[number];

/** Each curve's colour, in sRGB, shared by the editor and its histogram backdrop. */
export const CURVE_COLOURS: Record<CurveChannel, string> = {
    master: "#e6e6e6",
    red: "#f0443a",
    green: "#4cbf4f",
    blue: "#3f6ff0",
};

/** A control point, both axes in encoded tone from 0 to 1. */
export interface CurvePoint {
    x: number;
    y: number;
}

/** Each curve is sorted by x, and always starts at x = 0 and ends at x = 1. */
export type Curves = Record<CurveChannel, CurvePoint[]>;

/** Entries per channel in the baked table. Must match `curve_lut` in chunks/params.wgsl. */
export const CURVE_LUT_SIZE = 256;

/** Below this a point counts as sitting on the identity diagonal. */
const IDENTITY_TOLERANCE = 1e-6;

export function identityCurve(): CurvePoint[] {
    return [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
    ];
}

export function defaultCurves(): Curves {
    return {
        master: identityCurve(),
        red: identityCurve(),
        green: identityCurve(),
        blue: identityCurve(),
    };
}

export function cloneCurves(curves: Curves): Curves {
    return Object.fromEntries(
        CURVE_CHANNELS.map((channel) => [channel, curves[channel].map((p) => ({ ...p }))]),
    ) as Curves;
}

export function isIdentityCurve(points: CurvePoint[]): boolean {
    return points.every((p) => Math.abs(p.x - p.y) <= IDENTITY_TOLERANCE);
}

export function curvesEqual(a: Curves, b: Curves): boolean {
    return CURVE_CHANNELS.every(
        (channel) =>
            a[channel].length === b[channel].length &&
            a[channel].every((p, i) => p.x === b[channel][i].x && p.y === b[channel][i].y),
    );
}

/**
 * Monotone cubic interpolation (Fritsch–Carlson, with PCHIP's weighted harmonic
 * tangents). Passes through every point and never overshoots between two of
 * them, so a curve can only reach values its own points reach.
 */
export function curveEvaluator(points: CurvePoint[]): (x: number) => number {
    const n = points.length;
    const xs = points.map((p) => p.x);
    const ys = points.map((p) => p.y);

    const h: number[] = [];
    const d: number[] = [];
    for (let i = 0; i < n - 1; i++) {
        h.push(xs[i + 1] - xs[i]);
        d.push((ys[i + 1] - ys[i]) / h[i]);
    }

    const m = new Array<number>(n);
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for (let i = 1; i < n - 1; i++) {
        // A turning point or a flat run gets a flat tangent, which is what
        // keeps the curve from overshooting its neighbours.
        if (d[i - 1] * d[i] <= 0) {
            m[i] = 0;
        } else {
            const w1 = 2 * h[i] + h[i - 1];
            const w2 = h[i] + 2 * h[i - 1];
            m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
        }
    }

    return (x: number) => {
        if (x <= xs[0]) return ys[0];
        if (x >= xs[n - 1]) return ys[n - 1];

        let i = 0;
        while (x > xs[i + 1]) i++;

        const t = (x - xs[i]) / h[i];
        const t2 = t * t;
        const t3 = t2 * t;
        return (
            (2 * t3 - 3 * t2 + 1) * ys[i] +
            (t3 - 2 * t2 + t) * h[i] * m[i] +
            (-2 * t3 + 3 * t2) * ys[i + 1] +
            (t3 - t2) * h[i] * m[i + 1]
        );
    };
}

/**
 * The table the develop chain samples: per input level, the colour curve then
 * the master curve, as r, g, b and an unused fourth lane. Null when every curve
 * that applies is the identity, which lets the chain skip the step.
 *
 * `withColour` false leaves the colour curves out, as B&W does.
 */
export function bakeCurveLut(curves: Curves, withColour: boolean): Float32Array | null {
    const colourActive =
        withColour && (["red", "green", "blue"] as const).some((c) => !isIdentityCurve(curves[c]));
    if (!colourActive && isIdentityCurve(curves.master)) return null;

    const master = curveEvaluator(curves.master);
    const identity = (x: number) => x;
    const channels = (["red", "green", "blue"] as const).map((c) =>
        colourActive ? curveEvaluator(curves[c]) : identity,
    );

    const lut = new Float32Array(CURVE_LUT_SIZE * 4);
    for (let i = 0; i < CURVE_LUT_SIZE; i++) {
        const x = i / (CURVE_LUT_SIZE - 1);
        for (let c = 0; c < 3; c++) {
            lut[i * 4 + c] = master(channels[c](x));
        }
    }
    return lut;
}
