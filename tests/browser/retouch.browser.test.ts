// tests/browser/retouch.browser.test.ts
//
// The preview's half of the heal and clone strokes: retouchLayer.ts baking
// them into the source texture, keeping its mip levels in step, and putting
// the untouched image back when a stroke is undone. The pixel maths itself is
// checked against the export in src-tauri/tests/retouch.rs.

import { beforeAll, describe, expect, it } from "vitest";

import type { GPUImage } from "$lib/types/gpuTypes";
import type { Retouch } from "$lib/types/retouch";
import { placeRetouch } from "$lib/types/retouch";
import { generateMipmaps, sourceTexture } from "$lib/gpu/fullResolution";
import { createRetouchLayer } from "$lib/gpu/retouchLayer";
import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
import { findSource } from "$lib/gpu/retouchSearch";

let device: GPUDevice;

beforeAll(async () => {
    if (!navigator.gpu) throw new Error("WebGPU unavailable in this browser");
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no WebGPU adapter");
    device = await adapter.requestDevice();
});

const scratch = new DataView(new ArrayBuffer(4));

function toHalf(value: number): number {
    scratch.setFloat32(0, value, true);
    const bits = scratch.getUint32(0, true);
    const exponent = ((bits >>> 23) & 0xff) - 127 + 15;
    if (exponent <= 0) return 0;
    return (exponent << 10) | ((bits & 0x7fffff) >> 13);
}

/** An image whose every pixel holds its own position, so a copy shows where it came from. */
function positionImage(width: number, height: number, value = (x: number, y: number) => [x / width, y / height, 0.5]): GPUImage {
    const texture = sourceTexture(device, "Retouch Test", width, height);
    const words = new Uint16Array(width * height * 4);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const [r, g, b] = value(x, y);
            words.set([toHalf(r), toHalf(g), toHalf(b), toHalf(1)], (y * width + x) * 4);
        }
    }
    device.queue.writeTexture({ texture }, words, { bytesPerRow: width * 8 }, { width, height });
    generateMipmaps(device, texture);
    return { texture, width, height };
}

async function readLevel(texture: GPUTexture, level: number): Promise<{ width: number; words: Uint16Array }> {
    const width = Math.max(1, texture.width >> level);
    const height = Math.max(1, texture.height >> level);
    const bytesPerRow = Math.ceil((width * 8) / 256) * 256;
    const staging = device.createBuffer({ size: bytesPerRow * height, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const encoder = device.createCommandEncoder();
    encoder.copyTextureToBuffer({ texture, mipLevel: level }, { buffer: staging, bytesPerRow }, { width, height });
    device.queue.submit([encoder.finish()]);
    await staging.mapAsync(GPUMapMode.READ);
    const padded = new Uint16Array(staging.getMappedRange().slice(0));
    staging.destroy();
    const words = new Uint16Array(width * height * 4);
    for (let y = 0; y < height; y++) {
        words.set(padded.subarray((y * bytesPerRow) / 2, (y * bytesPerRow) / 2 + width * 4), y * width * 4);
    }
    return { width, words };
}

const at = (level: { width: number; words: Uint16Array }, x: number, y: number) =>
    Array.from(level.words.subarray((y * level.width + x) * 4, (y * level.width + x) * 4 + 4));

const clone = (x: number, y: number, dx: number, dy: number, size: number): Retouch => ({
    mode: "clone",
    points: [[x / size, y / size]],
    radius: 4 / size,
    feather: 0,
    opacity: 1,
    offset: [dx / size, dy / size],
});

describe("retouchLayer", () => {
    it("bakes a clone into the source, and its mip levels with it", async () => {
        const image = positionImage(128, 128);
        const before = await readLevel(image.texture, 0);
        const beforeMip = await readLevel(image.texture, 1);
        const layer = createRetouchLayer(device);

        expect(layer.sync(image, [clone(96, 64, -64, 0, 128)])).toBe(true);
        const after = await readLevel(image.texture, 0);
        expect(at(after, 96, 64)).toEqual(at(before, 32, 64));
        expect(at(after, 10, 10)).toEqual(at(before, 10, 10));
        const afterMip = await readLevel(image.texture, 1);
        expect(at(afterMip, 48, 32)).not.toEqual(at(beforeMip, 48, 32));
        expect(at(afterMip, 5, 5)).toEqual(at(beforeMip, 5, 5));

        // The same strokes again change nothing.
        expect(layer.sync(image, [clone(96, 64, -64, 0, 128)])).toBe(false);
        layer.destroy();
        image.texture.destroy();
    });

    it("puts the untouched image back when a stroke is taken away", async () => {
        const image = positionImage(128, 128);
        const before = await readLevel(image.texture, 0);
        const layer = createRetouchLayer(device);
        const first = clone(96, 64, -64, 0, 128);
        const second = clone(96, 20, -64, 0, 128);

        layer.sync(image, [first, second]);
        layer.sync(image, [first]);
        const after = await readLevel(image.texture, 0);
        expect(at(after, 96, 20)).toEqual(at(before, 96, 20));
        expect(at(after, 96, 64)).toEqual(at(before, 32, 64));

        layer.sync(image, []);
        const cleared = await readLevel(image.texture, 0);
        expect(Array.from(cleared.words)).toEqual(Array.from(before.words));
        layer.destroy();
        image.texture.destroy();
    });

    it("heals a speck back to the tone around it", async () => {
        // Flat grey, darker on the left, with a bright speck on the right.
        const image = positionImage(128, 64, (x, y) => {
            if (Math.abs(x - 96) < 3 && Math.abs(y - 32) < 3) return [0.9, 0.9, 0.9];
            return x < 48 ? [0.1, 0.1, 0.1] : [0.4, 0.4, 0.4];
        });
        const layer = createRetouchLayer(device);
        layer.sync(image, [{ mode: "heal", points: [[96 / 128, 32 / 64]], radius: 6 / 128, feather: 0, opacity: 1, offset: [-72 / 128, 0] }]);
        const after = await readLevel(image.texture, 0);
        expect(at(after, 96, 32)[0]).toBe(toHalf(0.4));
        layer.destroy();
        image.texture.destroy();
    });
});

describe("the histogram's overview", () => {
    it("takes strokes the way the preview does", async () => {
        // The overview is uploaded without mip levels, as Histogram.svelte does.
        const words = new Uint16Array(64 * 64 * 4);
        for (let i = 0; i < 64 * 64; i++) words.set([toHalf((i % 64) / 64), toHalf(0.5), toHalf(0.5), toHalf(1)], i * 4);
        const image = uploadRawPixelsToGPU(device, new Uint8Array(words.buffer), 64, 64);
        const before = await readLevel(image.texture, 0);
        const layer = createRetouchLayer(device);
        device.pushErrorScope("validation");
        expect(layer.sync(image, [clone(48, 32, -32, 0, 64)])).toBe(true);
        expect(await device.popErrorScope()).toBeNull();
        const after = await readLevel(image.texture, 0);
        expect(at(after, 48, 32)).toEqual(at(before, 16, 32));
        layer.destroy();
        image.texture.destroy();
    });
});

describe("findSource", () => {
    it("finds an offset that clears the stroke and matches what is around it", async () => {
        // Grey everywhere but a bright band above the stroke.
        const image = positionImage(128, 128, (_x, y) => (y < 40 ? [0.9, 0.9, 0.9] : [0.3, 0.3, 0.3]));
        const stroke = placeRetouch(
            { mode: "clone", points: [[0.5, 0.75]], radius: 4 / 128, feather: 0, opacity: 1, offset: [0, 0] },
            128,
            128,
        );
        const offset = await findSource(device, image, stroke);
        expect(offset).not.toBeNull();
        const [dx, dy] = offset!;
        expect(Math.hypot(dx, dy)).toBeGreaterThanOrEqual(2 * stroke.radius);
        // Not into the bright band.
        expect(96 + dy - 2 * stroke.radius).toBeGreaterThan(40);
        image.texture.destroy();
    });
});
