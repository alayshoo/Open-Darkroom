// tests/unit/viewport.test.ts
//
// Where the preview puts the image, and what one frame of the chain covers.

import { describe, expect, it } from "vitest";

import { FULL_FRAME, type Affine } from "$lib/types/crop";
import {
    MAX_ZOOM,
    SHARP_REACH,
    apply,
    apronFor,
    canvasToImage,
    canvasToOutput,
    clampCenter,
    compose,
    cropLayout,
    invert,
    levelFor,
    planFrame,
    scaleBy,
    visibleRegion,
    wholeLayout,
    zoomAbout,
    type Point,
    type Slot,
} from "$lib/utils/viewport";

const SLOT: Slot = { center: [500, 500], width: 1000, height: 1000 };
const CANVAS = [1000, 1000] as const;
const WHOLE = { kx: 1, ky: 1, offset: [0, 0] as Point };

function expectPoint(actual: Point, expected: Point) {
    expect(actual[0]).toBeCloseTo(expected[0], 6);
    expect(actual[1]).toBeCloseTo(expected[1], 6);
}

describe("affine maps", () => {
    const a: Affine = { origin: [3, -2], axisX: [0.5, 0.25], axisY: [-1, 2] };

    it("compose applies the inner map first", () => {
        const b = scaleBy(2, 3, 1, 1);
        expectPoint(apply(compose(a, b), [4, 5]), apply(a, apply(b, [4, 5])));
    });

    it("invert undoes the map", () => {
        expectPoint(apply(invert(a), apply(a, [7, -3])), [7, -3]);
    });
});

describe("cropLayout", () => {
    it("fits the image to the slot and centres it", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, null, null);
        expect(layout.fit).toBe(0.25);
        expect(layout.placement.scale).toBe(0.25);
        expectPoint(apply(canvasToImage(layout.placement), [500, 500]), [2000, 1000]);
        expect(layout.frame).toEqual({ width: 1000, height: 500 });
    });

    it("never fits a small image past 1:1", () => {
        expect(cropLayout(FULL_FRAME, 100, 50, SLOT, null, null).fit).toBe(1);
    });

    it("draws nothing outside the crop", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, null, null);
        const inside = (p: Point) => {
            const [u, v] = apply(layout.clip, p);
            return u >= 0 && u <= 1 && v >= 0 && v <= 1;
        };
        expect(inside([500, 500])).toBe(true);
        expect(inside([500, 240])).toBe(false);
        expect(inside([500, 260])).toBe(true);
    });

    it("at a zoom, puts the chosen centre over the middle of the slot", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [300, 200]);
        expectPoint(apply(canvasToImage(layout.placement), [500, 500]), [300, 200]);
        expectPoint(apply(canvasToImage(layout.placement), [501, 500]), [301, 200]);
    });

    it("keeps the centre on the image", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [-50, 9000]);
        expect(layout.placement.center).toEqual([0, 2000]);
        expect(clampCenter([10, 10], layout.placement.view)).toEqual([10, 10]);
    });
});

describe("wholeLayout", () => {
    const crop = { ...FULL_FRAME, cx: 0.25, w: 0.5, angle: 0.2 };
    const layout = wholeLayout(crop, 4000, 2000, SLOT);

    it("shades outside the crop box, laid on the upright frame", () => {
        const [x0, y0, x1, y1] = layout.keep!;
        const { width, height } = layout.frame;
        expect(x0).toBeCloseTo(500 - width / 2, 6);
        expect(x1).toBeCloseTo(500, 6);
        expect(y1 - y0).toBeCloseTo(height, 6);
    });

    it("leaves clear whatever lies past the rotated image", () => {
        const unit = (p: Point) => apply(layout.clip, p);
        const [u, v] = unit([500, 500]);
        expect(u).toBeCloseTo(0.5, 6);
        expect(v).toBeCloseTo(0.5, 6);
        const [cu, cv] = unit([2, 2]);
        expect(cu < 0 || cv < 0 || cu > 1 || cv > 1).toBe(true);
    });
});

describe("zoomAbout", () => {
    it("keeps the point under the cursor where it was", () => {
        const before = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 0.5, [2000, 1000]).placement;
        const cursor: Point = [800, 300];
        const center = zoomAbout(before, 2, cursor);
        const after = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 2, center).placement;
        expectPoint(apply(canvasToOutput(after), cursor), apply(canvasToOutput(before), cursor));
        expect(MAX_ZOOM).toBeGreaterThan(1);
    });
});

describe("levelFor", () => {
    const levels = [9504, 4752, 2376, 1188].map((width) => ({ width }));

    it("picks the coarsest level with a texel for every canvas pixel", () => {
        expect(levelFor(1, levels)).toBe(0);
        expect(levelFor(0.5, levels)).toBe(1);
        expect(levelFor(0.3, levels)).toBe(1);
        expect(levelFor(0.25, levels)).toBe(2);
        expect(levelFor(0.1, levels)).toBe(3);
        expect(levelFor(0.01, levels)).toBe(3);
    });

    it("stays at full resolution past 1:1", () => {
        expect(levelFor(4, levels)).toBe(0);
    });
});

describe("visibleRegion", () => {
    it("is the whole image when it fits", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, null, null);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        expect(visible.canvas.map(Math.round)).toEqual([0, 250, 1000, 750]);
        expect(visible.image.map(Math.round)).toEqual([0, 0, 4000, 2000]);
    });

    it("is what the canvas can show when zoomed in", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [2000, 1000]);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        expect(visible.canvas).toEqual([0, 0, 1000, 1000]);
        expect(visible.image.map(Math.round)).toEqual([1500, 500, 2500, 1500]);
    });

    it("is nothing once the image is off the canvas", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [2000, 1000]);
        expect(visibleRegion(layout, 1000, 1000, 4000, 2000)).not.toBeNull();
        const away = { ...layout, clip: compose(scaleBy(1, 1, -5000, 0), layout.clip) };
        expect(visibleRegion(away, 1000, 1000, 4000, 2000)).toBeNull();
    });
});

describe("planFrame", () => {
    it("at 1:1 lands every render pixel on a texel centre", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [2000.3, 1000.7]);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        const frame = planFrame(layout, visible, WHOLE, false, ...CANVAS);
        expect(frame.density).toBe(1);
        expect(frame.sample.axisX).toEqual([1, 0]);
        expect(frame.sample.axisY).toEqual([0, 1]);
        expect(Number.isInteger(frame.sample.origin[0])).toBe(true);
        expect(Number.isInteger(frame.sample.origin[1])).toBe(true);
    });

    it("covers what is visible plus the reach of the blurs", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 1, [2000, 1000]);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        const frame = planFrame(layout, visible, WHOLE, true, ...CANVAS);
        const apron = SHARP_REACH + 2;
        expect(frame.viewport).toEqual({ x: -apron, y: -apron, width: 1000 + 2 * apron, height: 1000 + 2 * apron });
    });

    it("past 1:1 renders at full resolution and leaves the enlarging to the last stage", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 4, [2000, 1000]);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        const frame = planFrame(layout, visible, WHOLE, false, ...CANVAS);
        expect(frame.density).toBe(0.25);
        expect(frame.viewport.width).toBe(250 + 4);
        expect(frame.sample.axisX).toEqual([1, 0]);
    });

    it("samples a detail texture from its own corner", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, 0.5, [2000, 1000]);
        const visible = visibleRegion(layout, ...CANVAS, 4000, 2000)!;
        const detail = { kx: 0.5, ky: 0.5, offset: [600, 200] as Point };
        const frame = planFrame(layout, visible, detail, false, ...CANVAS);
        // The render pixel over the middle of the slot shows image (2000, 1000),
        // which is level texel (1000, 500), so texel (400, 300) of the texture.
        const u: Point = [500 - frame.viewport.x, 500 - frame.viewport.y];
        expectPoint(apply(frame.sample, u), [400, 300]);
    });
});

describe("apronFor", () => {
    it("shrinks with the render resolution and vanishes without sharpening", () => {
        expect(apronFor(1, 1, true)).toBe(SHARP_REACH + 2);
        expect(apronFor(1, 0.25, true)).toBe(SHARP_REACH / 4 + 2);
        expect(apronFor(0.25, 4, true)).toBe(SHARP_REACH + 2);
        expect(apronFor(1, 1, false)).toBe(2);
    });
});
