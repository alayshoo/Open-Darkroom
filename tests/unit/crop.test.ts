import { describe, expect, it } from "vitest";
import {
    FULL_FRAME,
    cropInside,
    cropView,
    fitCrop,
    isFullFrame,
    turnCrop,
    type Crop,
} from "$lib/types/crop";

describe("cropView", () => {
    it("maps the full frame one to one", () => {
        expect(cropView(FULL_FRAME, 640, 480)).toEqual({
            width: 640,
            height: 480,
            origin: [0, 0],
            axisX: [1, 0],
            axisY: [0, 1],
        });
    });

    it("snaps an unrotated crop to whole source pixels", () => {
        const view = cropView({ cx: 0.4, cy: 0.5, w: 0.333, h: 0.5, angle: 0, quarterTurns: 0 }, 100, 10);
        expect(view.origin).toEqual([23, 3]);
        expect(view.width).toBe(34);
        expect(view.height).toBe(5);
    });

    it("never collapses to nothing, or leaves the source", () => {
        const view = cropView({ cx: 1, cy: 1, w: 0.0001, h: 0.0001, angle: 0, quarterTurns: 0 }, 50, 50);
        expect(view.width).toBe(1);
        expect(view.height).toBe(1);
        expect(view.origin).toEqual([49, 49]);
    });

    it("keeps a rotated crop centred where it was put", () => {
        const view = cropView({ cx: 0.5, cy: 0.5, w: 0.5, h: 0.5, angle: Math.PI / 6, quarterTurns: 0 }, 200, 100);
        const centreX = view.origin[0] + (view.width / 2) * view.axisX[0] + (view.height / 2) * view.axisY[0];
        const centreY = view.origin[1] + (view.width / 2) * view.axisX[1] + (view.height / 2) * view.axisY[1];
        expect(centreX).toBeCloseTo(100);
        expect(centreY).toBeCloseTo(50);
    });

    it("recognises the full frame", () => {
        expect(isFullFrame({ ...FULL_FRAME })).toBe(true);
        expect(isFullFrame({ ...FULL_FRAME, w: 0.9 })).toBe(false);
    });
});

describe("quarter turns", () => {
    it("swap the output's dimensions", () => {
        const view = cropView({ ...FULL_FRAME, quarterTurns: 1 }, 640, 480);
        expect([view.width, view.height]).toEqual([480, 640]);
    });

    // Output pixel (u, v) of a clockwise turn is source pixel (v, H - 1 - u).
    it("read every output pixel from a source texel centre", () => {
        const view = cropView({ ...FULL_FRAME, quarterTurns: 1 }, 6, 4);
        const at = (u: number, v: number) => [
            view.origin[0] + (u + 0.5) * view.axisX[0] + (v + 0.5) * view.axisY[0],
            view.origin[1] + (u + 0.5) * view.axisX[1] + (v + 0.5) * view.axisY[1],
        ];
        expect(at(0, 0)).toEqual([0.5, 3.5]);
        expect(at(3, 5)).toEqual([5.5, 0.5]);
    });

    it("carry the crop round with the image and back", () => {
        const crop: Crop = { cx: 0.3, cy: 0.6, w: 0.2, h: 0.5, angle: 0.1, quarterTurns: 0 };
        let turned = crop;
        for (let i = 0; i < 4; i++) turned = turnCrop(turned, true);
        expect(turned.cx).toBeCloseTo(crop.cx);
        expect(turned.cy).toBeCloseTo(crop.cy);
        expect(turned.quarterTurns).toBe(0);
        expect(turnCrop(turnCrop(crop, true), false)).toEqual(crop);
    });
});

describe("keeping the crop on the image", () => {
    it("counts the unrotated full frame as inside", () => {
        expect(cropInside(FULL_FRAME, 300, 200)).toBe(true);
    });

    it("finds the full frame no longer fits once the image turns", () => {
        expect(cropInside({ ...FULL_FRAME, angle: 0.1 }, 300, 200)).toBe(false);
    });

    it("shrinks a rotated crop until its corners just touch the image", () => {
        const fitted = fitCrop({ ...FULL_FRAME, angle: 0.2 }, 300, 200);
        expect(cropInside(fitted, 300, 200)).toBe(true);
        expect(fitted.w / fitted.h).toBeCloseTo(1);
        const larger = { ...fitted, w: fitted.w * 1.01, h: fitted.h * 1.01 };
        expect(cropInside(larger, 300, 200)).toBe(false);
    });

    it("leaves a crop that already fits alone", () => {
        const small: Crop = { cx: 0.5, cy: 0.5, w: 0.3, h: 0.3, angle: 0.1, quarterTurns: 0 };
        expect(fitCrop(small, 300, 200)).toEqual(small);
    });

    it("pulls a centre that has left the image back onto it", () => {
        const stray: Crop = { cx: 0.01, cy: 0.01, w: 0.2, h: 0.2, angle: 0.6, quarterTurns: 0 };
        const fitted = fitCrop(stray, 300, 200);
        expect(cropInside(fitted, 300, 200)).toBe(true);
        expect(fitted.w).toBeGreaterThan(0);
    });
});

// The same case is asserted in src-tauri/tests/crop.rs, so the two
// implementations cannot drift apart on a turned and rotated crop.
it("maps a turned and rotated crop as the backend does", () => {
    const view = cropView({ cx: 0.4, cy: 0.55, w: 0.3, h: 0.2, angle: 0.2, quarterTurns: 1 }, 100, 60);
    expect([view.width, view.height]).toEqual([18, 20]);
    expect(view.origin[0]).toBeCloseTo(48.0797071, 6);
    expect(view.origin[1]).toBeCloseTo(45.6943453, 6);
    expect(view.axisX[0]).toBeCloseTo(-0.1986693, 6);
    expect(view.axisX[1]).toBeCloseTo(-0.9800666, 6);
    expect(view.axisY[0]).toBeCloseTo(0.9800666, 6);
    expect(view.axisY[1]).toBeCloseTo(-0.1986693, 6);
});
