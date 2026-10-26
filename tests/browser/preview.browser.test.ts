// tests/browser/preview.browser.test.ts
//
// The preview as the app draws it: the full-resolution stream into a mipmapped
// texture, and the renderer drawing a region of it onto a real canvas. What
// matters here is geometry — that 1:1 shows the source's own pixels wherever
// the view is moved to — and that moving the view re-runs no more of the chain
// than it has to.

import { beforeAll, describe, expect, it } from "vitest";

import type { GPUCanvasLink, GPUSession, Renderer } from "$lib/types/gpuTypes";
import { defaultSlidersRGB } from "$lib/types/imgParameters";
import { FULL_FRAME, type Crop } from "$lib/types/crop";
import { SRGB_TO_WORKING } from "$lib/gpu/workingSpace";
import { createRenderer } from "$lib/gpu/renderer";
import { ROWS_HEADER_BYTES, loadFullResolution, uploadSource } from "$lib/gpu/fullResolution";
import {
    PAN_MARGIN,
    apply,
    canvasToImage,
    cropLayout,
    viewFrame,
    type Point,
    type Slot,
} from "$lib/utils/viewport";

let gpu: GPUSession;

beforeAll(async () => {
    if (!navigator.gpu) throw new Error("WebGPU unavailable in this browser");
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no WebGPU adapter");
    gpu = {
        device: await adapter.requestDevice(),
        format: "rgba8unorm",
        displaySpace: "srgb",
        canTimestamp: false,
        adapterInfo: null,
        adapterFeatures: [],
    };
});

// ── f16 ───────────────────────────────────────────────────────────────────────

const scratch = new DataView(new ArrayBuffer(4));

function toHalf(value: number): number {
    scratch.setFloat32(0, value, true);
    const bits = scratch.getUint32(0, true);
    const sign = (bits >>> 16) & 0x8000;
    const exponent = ((bits >>> 23) & 0xff) - 127 + 15;
    const mantissa = bits & 0x7fffff;
    if (exponent <= 0) return sign;
    return sign | (exponent << 10) | (mantissa >> 13);
}

function fromHalf(h: number): number {
    const exponent = (h >> 10) & 0x1f;
    const mantissa = h & 0x3ff;
    const sign = h & 0x8000 ? -1 : 1;
    if (exponent === 0) return sign * mantissa * 2 ** -24;
    return sign * (1 + mantissa / 1024) * 2 ** (exponent - 15);
}

const srgbToLinear = (s: number) => (s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4));

/** Every pixel far from its neighbours, so a one-pixel shift is a large error. */
function scatterChart(width: number, height: number): Uint8Array {
    const rgb = new Uint8Array(width * height * 3);
    for (let i = 0; i < width * height; i++) {
        rgb[i * 3] = (i * 149 + 11) & 0xff;
        rgb[i * 3 + 1] = (i * 83 + 197) & 0xff;
        rgb[i * 3 + 2] = (i * 211 + 61) & 0xff;
    }
    return rgb;
}

function linearise(rgb: Uint8Array): Uint8Array {
    const out = new Uint8Array((rgb.length / 3) * 8);
    const view = new DataView(out.buffer);
    for (let i = 0; i < rgb.length / 3; i++) {
        for (let c = 0; c < 3; c++) view.setUint16(i * 8 + c * 2, toHalf(srgbToLinear(rgb[i * 3 + c] / 255)), true);
        view.setUint16(i * 8 + 6, toHalf(1), true);
    }
    return out;
}

/** One level of an rgba16float texture, as raw f16 words. */
async function readLevel(texture: GPUTexture, level: number): Promise<{ width: number; height: number; words: Uint16Array }> {
    const width = Math.max(1, texture.width >> level);
    const height = Math.max(1, texture.height >> level);
    const bytesPerRow = Math.ceil((width * 8) / 256) * 256;
    const staging = gpu.device.createBuffer({
        size: bytesPerRow * height,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
    const encoder = gpu.device.createCommandEncoder();
    encoder.copyTextureToBuffer({ texture, mipLevel: level }, { buffer: staging, bytesPerRow }, { width, height });
    gpu.device.queue.submit([encoder.finish()]);
    await staging.mapAsync(GPUMapMode.READ);
    const padded = new Uint8Array(staging.getMappedRange());
    const words = new Uint16Array(width * height * 4);
    for (let y = 0; y < height; y++) {
        words.set(new Uint16Array(padded.buffer.slice(y * bytesPerRow, y * bytesPerRow + width * 8)), y * width * 4);
    }
    staging.unmap();
    staging.destroy();
    return { width, height, words };
}

// ── The full-resolution stream ────────────────────────────────────────────────

describe("the full-resolution stream", () => {
    const WIDTH = 100;
    const HEIGHT = 37;
    const pixels = linearise(scatterChart(WIDTH, HEIGHT));

    // Serves bands of five rows, whatever is asked for, as full_resolution.rs
    // lays them out.
    function serve(requests: number[], width = WIDTH, height = HEIGHT, image = pixels) {
        return async (_maxEdge: number, y: number): Promise<ArrayBuffer> => {
            requests.push(y);
            const rows = Math.min(5, height - y);
            const buffer = new ArrayBuffer(ROWS_HEADER_BYTES + width * rows * 8);
            const view = new DataView(buffer);
            [width, height, y, rows].forEach((v, i) => view.setUint32(i * 4, v, true));
            new Uint8Array(buffer, ROWS_HEADER_BYTES).set(image.subarray(y * width * 8, (y + rows) * width * 8));
            return buffer;
        };
    }

    it("puts every band where it belongs", async () => {
        const requests: number[] = [];
        const image = (await loadFullResolution(gpu.device, WIDTH, serve(requests), () => false))!;
        expect(requests).toEqual([0, 5, 10, 15, 20, 25, 30, 35]);
        expect([image.width, image.height]).toEqual([WIDTH, HEIGHT]);

        const top = await readLevel(image.texture, 0);
        expect(new Uint8Array(top.words.buffer)).toEqual(pixels);
        image.texture.destroy();
    });

    it("fills each mip level with the mean of the four texels above it", async () => {
        const [w, h] = [64, 36];
        const even = linearise(scatterChart(w, h));
        const image = (await loadFullResolution(gpu.device, w, serve([], w, h, even), () => false))!;
        expect(image.texture.mipLevelCount).toBe(7);

        const top = await readLevel(image.texture, 0);
        const next = await readLevel(image.texture, 1);
        expect([next.width, next.height]).toEqual([32, 18]);
        for (const [x, y] of [[0, 0], [17, 9], [31, 17]]) {
            for (let c = 0; c < 3; c++) {
                const at = (px: number, py: number) => fromHalf(top.words[(py * w + px) * 4 + c]);
                const mean = (at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1)) / 4;
                expect(fromHalf(next.words[(y * 32 + x) * 4 + c])).toBeCloseTo(mean, 2);
            }
        }
        image.texture.destroy();
    });

    it("spreads an odd level over the whole of its parent", async () => {
        const image = (await loadFullResolution(gpu.device, WIDTH, serve([]), () => false))!;
        const top = await readLevel(image.texture, 0);
        const next = await readLevel(image.texture, 1);
        expect([next.width, next.height]).toEqual([50, 18]);
        // The last row of the level reads the parent's last two rows, not the
        // pair a 2:1 map would end on and leave the final row unread.
        const y = 17;
        const py = ((y + 0.5) * HEIGHT) / 18 - 0.5;
        const [r0, f] = [Math.floor(py), py - Math.floor(py)];
        for (let c = 0; c < 3; c++) {
            const at = (px: number, row: number) => fromHalf(top.words[(row * WIDTH + px) * 4 + c]);
            const row = (r: number) => (at(20, r) + at(21, r)) / 2;
            const expected = row(r0) * (1 - f) + row(r0 + 1) * f;
            expect(fromHalf(next.words[(y * 50 + 10) * 4 + c])).toBeCloseTo(expected, 2);
        }
        image.texture.destroy();
    });

    it("stops when it is cancelled", async () => {
        const requests: number[] = [];
        let cancelled = false;
        const fetch = serve(requests);
        const result = await loadFullResolution(gpu.device, WIDTH, async (m, y) => {
            const band = await fetch(m, y);
            cancelled = y >= 10;
            return band;
        }, () => cancelled);
        expect(result).toBeNull();
        expect(requests.length).toBeLessThan(5);
    });
});

// ── The renderer ──────────────────────────────────────────────────────────────

describe("the preview renderer", () => {
    const WIDTH = 700;
    const HEIGHT = 161;
    const CANVAS_W = 120;
    const CANVAS_H = 90;
    const source = scatterChart(WIDTH, HEIGHT);
    const slot: Slot = { center: [CANVAS_W / 2, CANVAS_H / 2], width: CANVAS_W, height: CANVAS_H };

    // Presenting to a real canvas loses the device in headless Chromium, so the
    // renderer draws into a texture of the canvas's size and format instead.
    let target: GPUTexture;
    let renderer: Renderer;
    let passes: string[] = [];

    beforeAll(async () => {
        // Record the label of every render pass the renderer begins.
        const createEncoder = gpu.device.createCommandEncoder.bind(gpu.device);
        gpu.device.createCommandEncoder = (descriptor?: GPUCommandEncoderDescriptor) => {
            const encoder = createEncoder(descriptor);
            const begin = encoder.beginRenderPass.bind(encoder);
            encoder.beginRenderPass = (d: GPURenderPassDescriptor) => {
                passes.push(d.label ?? "");
                return begin(d);
            };
            return encoder;
        };

        target = gpu.device.createTexture({
            size: { width: CANVAS_W, height: CANVAS_H },
            format: gpu.format,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
        });
        const link = { canvasConfig: { getCurrentTexture: () => target } } as unknown as GPUCanvasLink;
        renderer = createRenderer(gpu, link);
        renderer.loadImage(uploadSource(gpu.device, linearise(source), WIDTH, HEIGHT), SRGB_TO_WORKING);
    });

    /** Show the image at `zoom` about `center`, and return what reached the canvas. */
    async function show(zoom: number | null, center: Point | null, crop: Crop = FULL_FRAME) {
        const layout = cropLayout(crop, WIDTH, HEIGHT, slot, zoom, center);
        passes = [];
        renderer.setFrame(viewFrame(layout, CANVAS_W, CANVAS_H, WIDTH, HEIGHT));
        renderer.requestRender();
        await new Promise((r) => requestAnimationFrame(r));
        await gpu.device.queue.onSubmittedWorkDone();
        await new Promise((r) => requestAnimationFrame(r));

        const recorded = [...passes];
        const bytesPerRow = Math.ceil((CANVAS_W * 4) / 256) * 256;
        const staging = gpu.device.createBuffer({
            size: bytesPerRow * CANVAS_H,
            usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
        });
        const encoder = gpu.device.createCommandEncoder();
        encoder.copyTextureToBuffer({ texture: target }, { buffer: staging, bytesPerRow }, { width: CANVAS_W, height: CANVAS_H });
        gpu.device.queue.submit([encoder.finish()]);
        await staging.mapAsync(GPUMapMode.READ);
        const padded = new Uint8Array(staging.getMappedRange());
        const rgba = new Uint8Array(CANVAS_W * CANVAS_H * 4);
        for (let y = 0; y < CANVAS_H; y++) {
            rgba.set(padded.subarray(y * bytesPerRow, y * bytesPerRow + CANVAS_W * 4), y * CANVAS_W * 4);
        }
        staging.unmap();
        staging.destroy();
        return { layout, rgba, passes: recorded };
    }

    /** The largest difference between the canvas and the source pixel it shows. */
    function worstAt1to1(shown: Awaited<ReturnType<typeof show>>): number {
        const toImage = canvasToImage(shown.layout.placement);
        let worst = 0;
        for (let y = 0; y < CANVAS_H; y++) {
            for (let x = 0; x < CANVAS_W; x++) {
                const [ix, iy] = apply(toImage, [x + 0.5, y + 0.5]).map(Math.floor);
                if (ix < 0 || iy < 0 || ix >= WIDTH || iy >= HEIGHT) continue;
                for (let c = 0; c < 3; c++) {
                    const delta = Math.abs(shown.rgba[(y * CANVAS_W + x) * 4 + c] - source[(iy * WIDTH + ix) * 3 + c]);
                    worst = Math.max(worst, delta);
                }
            }
        }
        return worst;
    }

    it("at 1:1 shows the source's own pixels", async () => {
        const shown = await show(1, [100, 70]);
        expect(shown.passes).toContain("View Sample Pass");
        expect(worstAt1to1(shown)).toBeLessThanOrEqual(2);
    });

    it("pans within the margin without re-running the chain, still pixel for pixel", async () => {
        const shown = await show(1, [117.4, 61.8]);
        expect(shown.passes).toEqual(["Colour Space Encode Pass"]);
        expect(worstAt1to1(shown)).toBeLessThanOrEqual(2);
    });

    it("renders a fresh region once a pan runs past it", async () => {
        // A fresh region about the starting point, which a change of
        // adjustments forces.
        renderer.setSliders({ ...defaultSlidersRGB });
        await show(1, [60, 45]);
        const shown = await show(1, [60 + PAN_MARGIN + 10, 45]);
        expect(shown.passes).toContain("View Sample Pass");
        expect(worstAt1to1(shown)).toBeLessThanOrEqual(2);
    });

    it("re-runs the chain when the adjustments change", async () => {
        await show(1, [100, 70]);
        renderer.setSliders({ ...defaultSlidersRGB });
        const shown = await show(1, [100, 70]);
        expect(shown.passes).toContain("Develop Pass");
    });

    it("draws nothing outside the image", async () => {
        // Fitted, the image is wider than the canvas's shape, with a band above
        // and below.
        const shown = await show(null, null);
        const [, y0, , y1] = shown.layout.clip!;
        expect(y0).toBeGreaterThan(2);
        const alpha = (x: number, y: number) => shown.rgba[(y * CANVAS_W + x) * 4 + 3];
        expect(alpha(60, 0)).toBe(0);
        expect(alpha(60, Math.floor(y0) - 1)).toBe(0);
        expect(alpha(60, Math.ceil(y1) + 1)).toBe(0);
        expect(alpha(60, 45)).toBe(255);
    });

    it("lets a zoom under way stand in, then settles at the new scale", async () => {
        await show(0.6, [120, 80]);
        const moving = await show(0.7, [120, 80]);
        expect(moving.passes).toEqual(["Colour Space Encode Pass"]);

        await new Promise((r) => setTimeout(r, 200));
        await new Promise((r) => requestAnimationFrame(r));
        await gpu.device.queue.onSubmittedWorkDone();
        // The settled frame was drawn by the renderer's own timer.
        const settled = await show(0.7, [120, 80]);
        expect(settled.passes).toEqual(["Colour Space Encode Pass"]);
    });

    it("at 1:1 on a quarter-turned crop still shows the source's own pixels", async () => {
        const turned: Crop = { ...FULL_FRAME, quarterTurns: 1 };
        renderer.setSliders({ ...defaultSlidersRGB });
        const shown = await show(1, [80, 300], turned);
        expect(worstAt1to1(shown)).toBeLessThanOrEqual(2);
        const moved = await show(1, [91.6, 287.2], turned);
        expect(moved.passes).toEqual(["Colour Space Encode Pass"]);
        expect(worstAt1to1(moved)).toBeLessThanOrEqual(2);
    });

    it("with the blurs on, a pan drawn from the held region matches a fresh render", async () => {
        renderer.setSliders({ ...defaultSlidersRGB, clarity: 60, usmAmount: 80 });
        await show(1, [300, 80]);
        const held = await show(1, [300 + PAN_MARGIN - 20, 80]);
        expect(held.passes).toEqual(["Colour Space Encode Pass"]);

        renderer.setSliders({ ...defaultSlidersRGB, clarity: 60, usmAmount: 80 });
        const fresh = await show(1, [300 + PAN_MARGIN - 20, 80]);
        expect(fresh.passes).toContain("Sharp Blur V Pass");
        expect(held.rgba).toEqual(fresh.rgba);
    });

    it("at half size filters the source rather than picking every other texel", async () => {
        // A one-pixel checker: point sampling at 2:1 would land on one colour
        // throughout, where filtering averages the two.
        const checker = new Uint8Array(WIDTH * HEIGHT * 3);
        for (let y = 0; y < HEIGHT; y++) {
            for (let x = 0; x < WIDTH; x++) checker.fill((x + y) % 2 ? 255 : 0, (y * WIDTH + x) * 3, (y * WIDTH + x) * 3 + 3);
        }
        renderer.loadImage(uploadSource(gpu.device, linearise(checker), WIDTH, HEIGHT), SRGB_TO_WORKING);
        const shown = await show(0.5, [300, 80]);
        // Linear 0.5 is sRGB 188.
        const centre = shown.rgba[(45 * CANVAS_W + 60) * 4];
        expect(Math.abs(centre - 188)).toBeLessThanOrEqual(3);
        renderer.loadImage(uploadSource(gpu.device, linearise(source), WIDTH, HEIGHT), SRGB_TO_WORKING);
    });
});
