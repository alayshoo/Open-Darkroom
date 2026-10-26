// tests/unit/viewport.test.ts
//
// Where the preview puts the image, and which region of it the chain renders.

import { describe, expect, it } from "vitest";

import { FULL_FRAME, type Affine } from "$lib/types/crop";
import {
    MAX_ZOOM,
    PAN_MARGIN,
    apply,
    canvasToChain,
    canvasToImage,
    canvasToOutput,
    chainToSource,
    clampCenter,
    compose,
    contains,
    cropLayout,
    imageInChain,
    invert,
    neededBox,
    planRegion,
    scaleBy,
    standsIn,
    validBox,
    viewFrame,
    visibleRegion,
    wholeLayout,
    zoomAbout,
    type ChainRegion,
    type Point,
    type Slot,
} from "$lib/utils/viewport";

const SLOT: Slot = { center: [500, 500], width: 1000, height: 1000 };

function expectPoint(actual: Point, expected: Point) {
    expect(actual[0]).toBeCloseTo(expected[0], 6);
    expect(actual[1]).toBeCloseTo(expected[1], 6);
}

const frameAt = (zoom: number | null, center: Point | null, canvas = 1000) =>
    viewFrame(cropLayout(FULL_FRAME, 4000, 2000, SLOT, zoom, center), canvas, canvas, 4000, 2000);

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

    it("clips to the crop on the canvas", () => {
        const layout = cropLayout(FULL_FRAME, 4000, 2000, SLOT, null, null);
        expect(layout.clip!.map((v) => Math.round(v))).toEqual([0, 250, 1000, 750]);
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

    it("leaves the rotated image's own edge to decide what is drawn", () => {
        expect(layout.clip).toBeNull();
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

describe("visibleRegion", () => {
    it("is the whole image when it fits", () => {
        const visible = visibleRegion(frameAt(null, null))!;
        expect(visible.canvas.map(Math.round)).toEqual([0, 250, 1000, 750]);
        expect(visible.image.map(Math.round)).toEqual([0, 0, 4000, 2000]);
    });

    it("is what the canvas can show when zoomed in", () => {
        const visible = visibleRegion(frameAt(1, [2000, 1000]))!;
        expect(visible.canvas).toEqual([0, 0, 1000, 1000]);
        expect(visible.image.map(Math.round)).toEqual([1500, 500, 2500, 1500]);
    });

    it("is nothing once the image is off the canvas", () => {
        const frame = frameAt(1, [2000, 1000]);
        const away = { ...frame, toImage: compose(frame.toImage, scaleBy(1, 1, 5000, 0)) };
        expect(visibleRegion(away)).toBeNull();
    });
});

describe("planRegion", () => {
    it("covers what is needed, a margin for panning, and the blurs' apron", () => {
        const needed = neededBox([1500, 500, 2500, 1500], 1);
        const region = planRegion(needed, 1, 40, PAN_MARGIN, 4000, 2000, 8192);
        expect(region.box).toEqual([
            1499 - PAN_MARGIN - 40,
            499 - PAN_MARGIN - 40,
            2501 + PAN_MARGIN + 40,
            1501 + PAN_MARGIN + 40,
        ]);
        expect(validBox(region)).toEqual([1499 - PAN_MARGIN, 499 - PAN_MARGIN, 2501 + PAN_MARGIN, 1501 + PAN_MARGIN]);
    });

    it("takes the margin no further than the image goes", () => {
        const needed = neededBox([0, 0, 1000, 2000], 0.5);
        const region = planRegion(needed, 0.5, 0, PAN_MARGIN, 4000, 2000, 8192);
        expect(region.box).toEqual([-1, -1, 501 + PAN_MARGIN, 1001]);
    });

    it("never outgrows the texture limit", () => {
        const region = planRegion([0, 0, 9000, 100], 1, 10, 0, 9000, 100, 4096);
        expect(region.box[2] - region.box[0]).toBe(4096);
    });

    it("lets a region at another scale stand in only within a factor of two", () => {
        const region: ChainRegion = { scale: 0.5, box: [0, 0, 10, 10], apron: 0 };
        expect(standsIn(region, 0.9)).toBe(true);
        expect(standsIn(region, 0.26)).toBe(true);
        expect(standsIn(region, 1.1)).toBe(false);
        expect(standsIn(region, 0.2)).toBe(false);
    });
});

describe("mapping the canvas onto the chain", () => {
    it("at 1:1 lands every canvas pixel on a chain texel centre", () => {
        const frame = frameAt(1, [2000.3, 1000.7]);
        const region = planRegion(neededBox(visibleRegion(frame)!.image, 1), 1, 0, PAN_MARGIN, 4000, 2000, 8192);
        const map = canvasToChain(frame, region);
        expect(map.axisX).toEqual([1, 0]);
        expect(map.axisY).toEqual([0, 1]);
        expect(map.origin.every(Number.isInteger)).toBe(true);
    });

    it("reaches the same image point the placement shows", () => {
        const frame = frameAt(0.5, [2000, 1000]);
        const region: ChainRegion = { scale: 0.5, box: [600, 200, 1400, 800], apron: 0 };
        // The middle of the slot shows image (2000, 1000): chain texel
        // (1000, 500) in the image, (400, 300) in the region.
        expectPoint(apply(canvasToChain(frame, region), [500, 500]), [400, 300]);
    });

    it("samples a full-resolution source on texel centres at 1:1", () => {
        const region: ChainRegion = { scale: 1, box: [300, 200, 900, 700], apron: 0 };
        const map = chainToSource(region, 4000, 2000, 4000, 2000);
        expect(map).toEqual({ origin: [300, 200], axisX: [1, 0], axisY: [0, 1] });
    });

    it("samples a smaller source in proportion", () => {
        const region: ChainRegion = { scale: 0.25, box: [100, 50, 300, 250], apron: 0 };
        // Chain texel 0 is image pixel 400, which is texel 200 of a half-size source.
        expectPoint(apply(chainToSource(region, 2000, 1000, 4000, 2000), [0, 0]), [200, 100]);
    });

    it("knows where the image ends within the region", () => {
        const region: ChainRegion = { scale: 0.5, box: [-10, 100, 500, 600], apron: 0 };
        expect(imageInChain(region, 4000, 2000)).toEqual([10, -100, 2010, 900]);
    });

    it("needs a fresh region once a pan runs past the margin", () => {
        const at = (x: number) => neededBox(visibleRegion(frameAt(1, [x, 1000]))!.image, 1);
        const region = planRegion(at(2000), 1, 0, PAN_MARGIN, 4000, 2000, 8192);
        expect(contains(validBox(region), at(2000 + PAN_MARGIN - 2))).toBe(true);
        expect(contains(validBox(region), at(2000 + PAN_MARGIN + 2))).toBe(false);
    });
});
