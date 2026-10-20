import { describe, expect, it } from "vitest";
import { FULL_FRAME, cropView, isFullFrame } from "$lib/types/crop";

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
        const view = cropView({ cx: 0.4, cy: 0.5, w: 0.333, h: 0.5, angle: 0 }, 100, 10);
        expect(view.origin).toEqual([23, 3]);
        expect(view.width).toBe(34);
        expect(view.height).toBe(5);
    });

    it("never collapses to nothing, or leaves the source", () => {
        const view = cropView({ cx: 1, cy: 1, w: 0.0001, h: 0.0001, angle: 0 }, 50, 50);
        expect(view.width).toBe(1);
        expect(view.height).toBe(1);
        expect(view.origin).toEqual([49, 49]);
    });

    it("keeps a rotated crop centred where it was put", () => {
        const view = cropView({ cx: 0.5, cy: 0.5, w: 0.5, h: 0.5, angle: Math.PI / 6 }, 200, 100);
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
