// tests/unit/tileSource.test.ts
//
// The other half of the tile contract: `build_tile_payload` in pyramid.rs writes
// these bytes, and pyramid.rs's own tests check that side. Assembly is driven
// against a stand-in device that records what reaches the GPU.

import { beforeAll, describe, expect, it, vi } from "vitest";

import {
    REGION_MARGIN,
    createTileSource,
    levelRegion,
    parseTilePayload,
    tilesCovering,
    type PyramidStructure,
} from "$lib/gpu/tileSource";
import type { Box } from "$lib/utils/viewport";

const STRUCTURE: PyramidStructure = {
    id: 1,
    tileSize: 4,
    levels: [
        { width: 10, height: 6 },
        { width: 5, height: 3 },
    ],
};

/** Tiles laid out as build_tile_payload writes them; pixel bytes are the tile's column. */
function payload(level: number, tiles: [number, number][]): ArrayBuffer {
    const size = STRUCTURE.levels[level];
    const dims = tiles.map(([c, r]) => [
        Math.min(4, size.width - c * 4),
        Math.min(4, size.height - r * 4),
    ]);
    const length = 4 + dims.reduce((n, [w, h]) => n + 16 + w * h * 8, 0);
    const buffer = new ArrayBuffer(length);
    const view = new DataView(buffer);
    view.setUint32(0, tiles.length, true);
    let at = 4;
    tiles.forEach(([c, r], i) => {
        const [w, h] = dims[i];
        [c, r, w, h].forEach((v, j) => view.setUint32(at + j * 4, v, true));
        at += 16;
        new Uint8Array(buffer, at, w * h * 8).fill(c + 1);
        at += w * h * 8;
    });
    return buffer;
}

function fakeDevice() {
    const writes: { origin: { x: number; y: number }; size: { width: number; height: number } }[] = [];
    const textures: { size: { width: number; height: number }; destroyed: boolean }[] = [];
    const device = {
        limits: { maxTextureDimension2D: 8192 },
        createTexture: (d: { size: { width: number; height: number } }) => {
            const t = { size: d.size, destroyed: false, destroy() { t.destroyed = true; } };
            textures.push(t);
            return t;
        },
        queue: {
            writeTexture: (dst: { origin: { x: number; y: number } }, _d: unknown, _l: unknown, size: { width: number; height: number }) =>
                writes.push({ origin: dst.origin, size }),
        },
    };
    return { device: device as unknown as GPUDevice, writes, textures };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeAll(() => {
    (globalThis as Record<string, unknown>).GPUTextureUsage ??= { TEXTURE_BINDING: 4, COPY_DST: 2 };
});

describe("parseTilePayload", () => {
    it("reads each tile behind its header", () => {
        const tiles = parseTilePayload(payload(0, [[2, 1], [0, 0]]));
        expect(tiles.map((t) => [t.column, t.row, t.width, t.height])).toEqual([
            [2, 1, 2, 2],
            [0, 0, 4, 4],
        ]);
        expect(tiles[0].pixels.length).toBe(2 * 2 * 8);
        expect(tiles[0].pixels.every((b) => b === 3)).toBe(true);
    });
});

describe("tilesCovering", () => {
    it("lists every tile a region touches", () => {
        expect(tilesCovering([3, 0, 5, 4], 4)).toEqual([[0, 0], [1, 0]]);
        expect(tilesCovering([4, 4, 8, 6], 4)).toEqual([[1, 1]]);
    });
});

describe("levelRegion", () => {
    it("scales to the level, grows by the margin and stays on the level", () => {
        expect(levelRegion([2, 2, 6, 4], STRUCTURE, 1, 0)).toEqual([1, 1, 3, 2]);
        expect(levelRegion([2, 2, 6, 4], STRUCTURE, 1, 10)).toEqual([0, 0, 5, 3]);
        expect(REGION_MARGIN).toBeGreaterThan(0);
    });
});

describe("createTileSource", () => {
    it("assembles a region once all its tiles arrive, each written to its place", async () => {
        const { device, writes, textures } = fakeDevice();
        const fetch = vi.fn(async (level: number, tiles: [number, number][]) => payload(level, tiles));
        const onDetail = vi.fn();
        const source = createTileSource(device, STRUCTURE, fetch, onDetail);

        const region: Box = [2, 1, 9, 6];
        source.request(0, region);
        expect(onDetail).not.toHaveBeenCalled();
        await flush();

        expect(onDetail).toHaveBeenCalledOnce();
        expect(source.detail?.region).toEqual(region);
        expect(textures[0].size).toEqual({ width: 7, height: 5 });
        expect(writes).toEqual([
            { origin: { x: 0, y: 0 }, size: { width: 2, height: 3 } },
            { origin: { x: 2, y: 0 }, size: { width: 4, height: 3 } },
            { origin: { x: 6, y: 0 }, size: { width: 1, height: 3 } },
            { origin: { x: 0, y: 3 }, size: { width: 2, height: 2 } },
            { origin: { x: 2, y: 3 }, size: { width: 4, height: 2 } },
            { origin: { x: 6, y: 3 }, size: { width: 1, height: 2 } },
        ]);
    });

    it("asks only for tiles it does not hold, and swaps the old texture out", async () => {
        const { device, textures } = fakeDevice();
        const fetch = vi.fn(async (level: number, tiles: [number, number][]) => payload(level, tiles));
        const source = createTileSource(device, STRUCTURE, fetch, () => {});

        source.request(0, [0, 0, 4, 4]);
        await flush();
        source.request(0, [0, 0, 6, 4]);
        await flush();

        expect(fetch.mock.calls.map((c) => c[1])).toEqual([[[0, 0]], [[1, 0]]]);
        expect(textures[0].destroyed).toBe(true);
        expect(textures[1].destroyed).toBe(false);
    });

    it("asks for nothing when what it holds already covers the region", async () => {
        const { device } = fakeDevice();
        const fetch = vi.fn(async (level: number, tiles: [number, number][]) => payload(level, tiles));
        const onDetail = vi.fn();
        const source = createTileSource(device, STRUCTURE, fetch, onDetail);

        source.request(1, [0, 0, 5, 3]);
        await flush();
        source.request(1, [1, 1, 3, 2]);
        await flush();

        expect(fetch).toHaveBeenCalledOnce();
        expect(onDetail).toHaveBeenCalledOnce();
    });

    it("drops a reply for an image that has since been closed", async () => {
        const { device } = fakeDevice();
        const fetch = vi.fn(async () => Promise.reject("pyramid:stale"));
        const onDetail = vi.fn();
        const error = vi.spyOn(console, "error").mockImplementation(() => {});
        const source = createTileSource(device, STRUCTURE, fetch, onDetail);

        source.request(0, [0, 0, 4, 4]);
        await flush();

        expect(onDetail).not.toHaveBeenCalled();
        expect(error).not.toHaveBeenCalled();
        error.mockRestore();
    });
});
