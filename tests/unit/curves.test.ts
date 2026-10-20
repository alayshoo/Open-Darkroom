// tests/unit/curves.test.ts

import { describe, expect, it } from "vitest";
import {
    CURVE_LUT_SIZE,
    bakeCurveLut,
    curveEvaluator,
    defaultCurves,
    identityCurve,
} from "$lib/types/curves";

describe("curveEvaluator", () => {
    it("is the identity for the default curve", () => {
        const f = curveEvaluator(identityCurve());
        for (const x of [0, 0.1, 0.5, 0.93, 1]) expect(f(x)).toBeCloseTo(x, 10);
    });

    it("passes through every point", () => {
        const points = [
            { x: 0, y: 0.1 },
            { x: 0.3, y: 0.2 },
            { x: 0.6, y: 0.9 },
            { x: 1, y: 0.95 },
        ];
        const f = curveEvaluator(points);
        for (const p of points) expect(f(p.x)).toBeCloseTo(p.y, 10);
    });

    it("never leaves the range between neighbouring points", () => {
        const points = [
            { x: 0, y: 0 },
            { x: 0.45, y: 0.05 },
            { x: 0.55, y: 0.95 },
            { x: 1, y: 1 },
        ];
        const f = curveEvaluator(points);
        for (let i = 0; i < points.length - 1; i++) {
            const lo = Math.min(points[i].y, points[i + 1].y);
            const hi = Math.max(points[i].y, points[i + 1].y);
            for (let t = 0; t <= 1; t += 0.01) {
                const y = f(points[i].x + t * (points[i + 1].x - points[i].x));
                expect(y).toBeGreaterThanOrEqual(lo - 1e-9);
                expect(y).toBeLessThanOrEqual(hi + 1e-9);
            }
        }
    });

    it("stays monotone on monotone points", () => {
        const f = curveEvaluator([
            { x: 0, y: 0 },
            { x: 0.2, y: 0.5 },
            { x: 0.25, y: 0.52 },
            { x: 1, y: 1 },
        ]);
        let previous = -Infinity;
        for (let x = 0; x <= 1; x += 0.001) {
            const y = f(x);
            expect(y).toBeGreaterThanOrEqual(previous - 1e-12);
            previous = y;
        }
    });
});

describe("bakeCurveLut", () => {
    it("is null while every curve is the identity", () => {
        expect(bakeCurveLut(defaultCurves(), true)).toBeNull();
    });

    it("ignores the colour curves when told to", () => {
        const curves = defaultCurves();
        curves.red = [
            { x: 0, y: 0.2 },
            { x: 1, y: 1 },
        ];
        expect(bakeCurveLut(curves, true)).not.toBeNull();
        expect(bakeCurveLut(curves, false)).toBeNull();
    });

    it("applies the colour curve before the master", () => {
        const curves = defaultCurves();
        // Red halves, master lifts the floor: master(red(x)) = 0.5 + 0.25x,
        // where the other order would give 0.25 + 0.25x.
        curves.red = [
            { x: 0, y: 0 },
            { x: 1, y: 0.5 },
        ];
        curves.master = [
            { x: 0, y: 0.5 },
            { x: 1, y: 1 },
        ];
        const lut = bakeCurveLut(curves, true)!;
        expect(lut).toHaveLength(CURVE_LUT_SIZE * 4);

        const last = (CURVE_LUT_SIZE - 1) * 4;
        expect(lut[0]).toBeCloseTo(0.5, 6);
        expect(lut[last]).toBeCloseTo(0.75, 6);
        // Green only sees the master.
        expect(lut[1]).toBeCloseTo(0.5, 6);
        expect(lut[last + 1]).toBeCloseTo(1, 6);
    });
});
