// tests/unit/fullResolution.test.ts
//
// The other half of the band contract: `build_rows_payload` in
// full_resolution.rs writes these bytes, and its own tests check that side.

import { describe, expect, it } from "vitest";

import { ROWS_HEADER_BYTES, bandRows, mipLevelCount, parseRows } from "$lib/gpu/fullResolution";
import { sharpnessReach } from "$lib/gpu/sharpnessVisibility";
import { defaultSlidersRGB } from "$lib/types/imgParameters";

describe("parseRows", () => {
    it("reads the image size and the band behind the header", () => {
        const buffer = new ArrayBuffer(ROWS_HEADER_BYTES + 3 * 2 * 8);
        const view = new DataView(buffer);
        [3, 9, 4, 2].forEach((v, i) => view.setUint32(i * 4, v, true));
        new Uint8Array(buffer, ROWS_HEADER_BYTES).fill(7);

        const band = parseRows(buffer);
        expect([band.width, band.height, band.y, band.rows]).toEqual([3, 9, 4, 2]);
        expect(band.pixels.byteOffset).toBe(ROWS_HEADER_BYTES);
        expect(band.pixels.length).toBe(3 * 2 * 8);
        expect(band.pixels.every((b) => b === 7)).toBe(true);
    });
});

describe("bandRows", () => {
    it("keeps a band to about 32 MB", () => {
        expect(bandRows(9504) * 9504 * 8).toBeLessThanOrEqual(32 * 1024 * 1024);
        expect((bandRows(9504) + 1) * 9504 * 8).toBeGreaterThan(32 * 1024 * 1024);
    });

    it("always sends at least a row", () => {
        expect(bandRows(10_000_000)).toBe(1);
    });
});

describe("mipLevelCount", () => {
    it("runs down to a single texel along the longer edge", () => {
        expect(mipLevelCount(1, 1)).toBe(1);
        expect(mipLevelCount(4000, 2000)).toBe(12);
        expect(mipLevelCount(4096, 10)).toBe(13);
    });
});

describe("sharpnessReach", () => {
    it("is nothing while the sharpness stage is skipped", () => {
        expect(sharpnessReach(defaultSlidersRGB, 1, 6000, 4000)).toBe(0);
    });

    it("follows the widest band, brought down to the render scale", () => {
        const clarity = { ...defaultSlidersRGB, clarity: 50 };
        // Clarity's sigma on a 4000-pixel short edge is 40.
        expect(sharpnessReach(clarity, 1, 6000, 4000)).toBe(120);
        expect(sharpnessReach(clarity, 0.5, 6000, 4000)).toBe(60);
    });

    it("never passes the blur's own widest radius", () => {
        const clarity = { ...defaultSlidersRGB, clarity: 50 };
        expect(sharpnessReach(clarity, 1, 20000, 20000)).toBe(180);
    });
});
