// tests/unit/autoLevels.test.ts

import { describe, expect, it } from "vitest";

import { autoLevels, channelLevels } from "$lib/utils/autoLevels";

function histWith(counts: Record<number, number>) {
    const hist = new Uint32Array(256);
    for (const [bin, count] of Object.entries(counts)) hist[Number(bin)] = count;
    return hist;
}

describe("channelLevels", () => {
    it("brackets the occupied bins", () => {
        const hist = new Uint32Array(256);
        hist.fill(1000, 60, 120);
        const { black, white } = channelLevels(hist, 0)!;
        expect(black).toBeCloseTo((60 / 256) * 255, 3);
        expect(white).toBeCloseTo((120 / 256) * 255, 3);
    });

    it("ignores a sliver of outliers at either end", () => {
        const hist = histWith({ 0: 1, 255: 1 });
        hist.fill(1000, 60, 120);
        const { black, white } = channelLevels(hist)!;
        expect(black).toBeGreaterThan(59);
        expect(white).toBeLessThan(121);
    });

    it("keeps the points apart on a single spike", () => {
        const { black, white } = channelLevels(histWith({ 255: 5000 }))!;
        expect(white - black).toBeGreaterThanOrEqual(1 - 1e-9);
        expect(white).toBeLessThanOrEqual(255);
    });

    it("has nothing to say about an empty channel", () => {
        expect(channelLevels(new Uint32Array(256))).toBeNull();
    });
});

describe("autoLevels", () => {
    it("sets each channel on its own and leaves an empty one alone", () => {
        const r = new Uint32Array(256).fill(10, 100, 200);
        const g = new Uint32Array(256).fill(10, 50, 150);
        const b = new Uint32Array(256);
        const levels = autoLevels({ r, g, b });
        expect(levels.redBlackPoint).toBeLessThan(levels.redWhitePoint!);
        expect(levels.greenWhitePoint).toBeLessThan(levels.redWhitePoint!);
        expect(levels).not.toHaveProperty("blueBlackPoint");
        expect(levels).not.toHaveProperty("redGamma");
    });
});
