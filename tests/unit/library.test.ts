// tests/unit/library.test.ts
//
// The pure parts of the library: how a roll is summed up, what a frame field
// offers before anything is logged, and where the flying image lands.

import { describe, expect, it } from "vitest";

import { framePlaceholder, rollSummary } from "$lib/types/rollInfo";
import { fitRect } from "$lib/library/flight";
import type { ImageMetadata } from "$lib/types/imageMetadata";

const meta: ImageMetadata = {
    fileName: "a.tif", make: null, model: null, lens: null, focalLength: null,
    aperture: 5.6, exposureTime: [1, 250], iso: null, exposureBias: null, dateTaken: null,
};

describe("rollSummary", () => {
    it("names the stock and format, and the speed only when it was not shot at box", () => {
        expect(rollSummary({})).toBe("");
        expect(rollSummary({ filmStock: "Kodak Portra 400", format: "35mm", boxIso: "400", ratedIso: "400" })).toBe(
            "Kodak Portra 400 · 35mm",
        );
        expect(rollSummary({ filmStock: "Ilford HP5 Plus", boxIso: "400", ratedIso: "1600", pushPull: "+2" })).toBe(
            "Ilford HP5 Plus · @ 1600 · +2",
        );
    });
});

describe("framePlaceholder", () => {
    it("offers what the file recorded, and falls back to an example", () => {
        expect(framePlaceholder("aperture", meta)).toBe("f/5.6");
        expect(framePlaceholder("shutter", meta)).toBe("1/250 s");
        expect(framePlaceholder("aperture", null)).toBe("f/8");
        expect(framePlaceholder("location", meta)).toBeUndefined();
    });
});

describe("fitRect", () => {
    const box = { left: 100, top: 50, width: 800, height: 600 };

    it("fills the width of a box taller than the image, centred", () => {
        expect(fitRect(2, box)).toEqual({ left: 100, top: 150, width: 800, height: 400 });
    });

    it("fills the height of a box wider than the image, centred", () => {
        expect(fitRect(0.5, box)).toEqual({ left: 350, top: 50, width: 300, height: 600 });
    });
});
