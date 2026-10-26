// src/lib/gpu/tileSource.ts
//
// Streams tiles of the backend's pyramid and assembles them into one texture
// covering the part of a level the preview needs. Mirrors pyramid.rs.

import type { GPUImage } from "$lib/types/gpuTypes";
import type { Box } from "$lib/utils/viewport";

export interface LevelSize {
    width: number;
    height: number;
}

export interface PyramidStructure {
    id: number;
    tileSize: number;
    // Finest first: level 0 is the full-resolution image.
    levels: LevelSize[];
}

export interface Tile {
    column: number;
    row: number;
    width: number;
    height: number;
    // Linear RGBA f16 LE, `width` × `height` of them.
    pixels: Uint8Array;
}

/** A texture holding `region` of one level, in that level's pixels. */
export interface DetailSource {
    level: number;
    region: Box;
    image: GPUImage;
}

// Must match TILE_HEADER_BYTES and STALE_IMAGE in pyramid.rs.
const TILE_HEADER_BYTES = 16;
export const STALE_IMAGE = "pyramid:stale";

const BYTES_PER_PIXEL = 8;

/** Tiles asked for in one round trip, so no single payload grows unbounded. */
const TILES_PER_REQUEST = 16;

/** Decoded tiles kept on the CPU, least recently used dropped first. */
const CACHE_BUDGET_BYTES = 256 * 1024 * 1024;

/** How far past what is needed a detail region reaches, so a short pan needs nothing new. */
export const REGION_MARGIN = 256;

export function parseTilePayload(buffer: ArrayBuffer): Tile[] {
    const view = new DataView(buffer);
    const count = view.getUint32(0, true);
    const tiles: Tile[] = [];
    let at = 4;
    for (let i = 0; i < count; i++) {
        const [column, row, width, height] = [0, 4, 8, 12].map((o) => view.getUint32(at + o, true));
        at += TILE_HEADER_BYTES;
        const length = width * height * BYTES_PER_PIXEL;
        tiles.push({ column, row, width, height, pixels: new Uint8Array(buffer, at, length) });
        at += length;
    }
    return tiles;
}

/** The [column, row] of every tile `region` touches. */
export function tilesCovering(region: Box, tileSize: number): [number, number][] {
    const tiles: [number, number][] = [];
    for (let row = Math.floor(region[1] / tileSize); row * tileSize < region[3]; row++) {
        for (let col = Math.floor(region[0] / tileSize); col * tileSize < region[2]; col++) {
            tiles.push([col, row]);
        }
    }
    return tiles;
}

/**
 * The region of `level` covering `image` (in full-resolution pixels), grown by
 * `margin` level pixels, in whole pixels and clamped to the level.
 */
export function levelRegion(image: Box, structure: PyramidStructure, level: number, margin: number): Box {
    const full = structure.levels[0];
    const size = structure.levels[level];
    const kx = size.width / full.width;
    const ky = size.height / full.height;
    return [
        Math.max(0, Math.floor(image[0] * kx) - margin),
        Math.max(0, Math.floor(image[1] * ky) - margin),
        Math.min(size.width, Math.ceil(image[2] * kx) + margin),
        Math.min(size.height, Math.ceil(image[3] * ky) + margin),
    ];
}

export type FetchTiles = (level: number, tiles: [number, number][]) => Promise<ArrayBuffer>;

export interface TileSource {
    // Ask for `region` of `level`; `onDetail` fires once it is on the GPU.
    request: (level: number, region: Box) => void;
    readonly detail: DetailSource | null;
    destroy: () => void;
}

export function createTileSource(
    device: GPUDevice,
    structure: PyramidStructure,
    fetchTiles: FetchTiles,
    onDetail: (detail: DetailSource) => void,
): TileSource {
    const cache = new Map<string, Tile>();
    let cachedBytes = 0;
    const inFlight = new Set<string>();
    let wanted: { level: number; region: Box } | null = null;
    let detail: DetailSource | null = null;
    let destroyed = false;

    const key = (level: number, [col, row]: [number, number]) => `${level}/${col}/${row}`;

    function touch(k: string): Tile | undefined {
        const tile = cache.get(k);
        if (tile) {
            cache.delete(k);
            cache.set(k, tile);
        }
        return tile;
    }

    function store(k: string, tile: Tile) {
        cache.set(k, tile);
        cachedBytes += tile.pixels.byteLength;
        for (const [old, t] of cache) {
            if (cachedBytes <= CACHE_BUDGET_BYTES) break;
            cache.delete(old);
            cachedBytes -= t.pixels.byteLength;
        }
    }

    const covers = (have: { level: number; region: Box } | null, level: number, region: Box) =>
        !!have &&
        have.level === level &&
        have.region[0] <= region[0] &&
        have.region[1] <= region[1] &&
        have.region[2] >= region[2] &&
        have.region[3] >= region[3];

    function request(level: number, region: Box) {
        if (destroyed || covers(detail, level, region) || covers(wanted, level, region)) return;
        const limit = device.limits.maxTextureDimension2D;
        if (region[2] - region[0] > limit || region[3] - region[1] > limit) return;

        wanted = { level, region };
        const missing = tilesCovering(region, structure.tileSize).filter((t) => {
            const k = key(level, t);
            return !cache.has(k) && !inFlight.has(k);
        });
        for (let i = 0; i < missing.length; i += TILES_PER_REQUEST) {
            fetchBatch(level, missing.slice(i, i + TILES_PER_REQUEST));
        }
        assemble();
    }

    function fetchBatch(level: number, tiles: [number, number][]) {
        tiles.forEach((t) => inFlight.add(key(level, t)));
        fetchTiles(level, tiles)
            .then((buffer) => {
                if (destroyed) return;
                for (const tile of parseTilePayload(buffer)) {
                    store(key(level, [tile.column, tile.row]), tile);
                }
            })
            .catch((e) => {
                if (e !== STALE_IMAGE) console.error("Failed to fetch tiles:", e);
            })
            .finally(() => {
                tiles.forEach((t) => inFlight.delete(key(level, t)));
                assemble();
            });
    }

    // Uploads the wanted region once every tile of it is to hand.
    function assemble() {
        if (destroyed || !wanted) return;
        const { level, region } = wanted;
        const tiles = tilesCovering(region, structure.tileSize).map((t) => touch(key(level, t)));
        if (tiles.some((t) => !t)) return;

        const width = region[2] - region[0];
        const height = region[3] - region[1];
        const texture = device.createTexture({
            label: `Pyramid Level ${level}`,
            size: { width, height },
            format: "rgba16float",
            usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
        });

        const size = structure.tileSize;
        for (const tile of tiles as Tile[]) {
            const tx = tile.column * size;
            const ty = tile.row * size;
            const x0 = Math.max(region[0], tx);
            const y0 = Math.max(region[1], ty);
            const x1 = Math.min(region[2], tx + tile.width);
            const y1 = Math.min(region[3], ty + tile.height);
            device.queue.writeTexture(
                { texture, origin: { x: x0 - region[0], y: y0 - region[1] } },
                tile.pixels,
                {
                    offset: ((y0 - ty) * tile.width + (x0 - tx)) * BYTES_PER_PIXEL,
                    bytesPerRow: tile.width * BYTES_PER_PIXEL,
                },
                { width: x1 - x0, height: y1 - y0 },
            );
        }

        const previous = detail;
        detail = { level, region, image: { texture, width, height } };
        wanted = null;
        onDetail(detail);
        previous?.image.texture.destroy();
    }

    function destroy() {
        destroyed = true;
        detail?.image.texture.destroy();
        detail = null;
        cache.clear();
    }

    return {
        request,
        get detail() {
            return detail;
        },
        destroy,
    };
}
