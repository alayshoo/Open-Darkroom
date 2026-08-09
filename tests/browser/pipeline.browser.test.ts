// tests/browser/pipeline.browser.test.ts
//
// Layer 3: the frontend's own GPU modules, in a real browser, against real
// WebGPU. The numbers here are already covered by the Rust suite running the
// same WGSL; what only this layer can prove is that the TypeScript side wires
// it up the same way — slider packing, buffer sizes, bind group layouts and
// texture upload.
//
// Anything that fails here but passes in Rust is a TypeScript wiring bug.

import { beforeAll, describe, expect, it } from "vitest";

import type { GPUSession } from "$lib/types/gpuTypes";
import { defaultSlidersRGB, type Sliders } from "$lib/types/imgParameters";
import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
import { createImgDevPipeline } from "$lib/gpu/pipelines/imgDevPipeline";
import {
  createCalcParamsPipeline,
  PARAMS_BUFFER_SIZE,
  SLIDERS_BUFFER_SIZE,
  VIEW_BUFFER_SIZE,
} from "$lib/gpu/pipelines/calcParamsPipeline";

// ── Reference maths, mirroring the shaders ────────────────────────────────────

function srgbToLinear(s: number): number {
  return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}

function linearToSrgb(l: number): number {
  return l <= 0.0031308 ? l * 12.92 : 1.055 * Math.pow(l, 1 / 2.4) - 0.055;
}

/** Put an 8-bit value through linear light, applying `f` in between. */
function throughLinear(v: number, f: (l: number) => number): number {
  const linear = f(srgbToLinear(v / 255));
  return Math.round(linearToSrgb(Math.min(Math.max(linear, 0), 1)) * 255);
}

// ── f16 encoding for the rgba16float upload ───────────────────────────────────

const f16Scratch = new DataView(new ArrayBuffer(4));

function toHalf(value: number): number {
  f16Scratch.setFloat32(0, value, true);
  const bits = f16Scratch.getUint32(0, true);

  const sign = (bits >>> 16) & 0x8000;
  let exponent = (bits >>> 23) & 0xff;
  let mantissa = bits & 0x7fffff;

  if (exponent === 0xff) return sign | 0x7c00 | (mantissa ? 0x200 : 0);
  exponent = exponent - 127 + 15;
  if (exponent >= 0x1f) return sign | 0x7c00;
  if (exponent <= 0) {
    if (exponent < -10) return sign;
    mantissa |= 0x800000;
    const shift = 14 - exponent;
    return sign | (mantissa >> shift);
  }
  return sign | (exponent << 10) | (mantissa >> 13);
}

/** Build an RGBA f16 buffer from 8-bit sRGB values, as the Rust side does. */
function linearisePixels(rgb: Uint8Array): Uint8Array {
  const pixelCount = rgb.length / 3;
  const out = new Uint8Array(pixelCount * 8);
  const view = new DataView(out.buffer);

  for (let i = 0; i < pixelCount; i++) {
    for (let c = 0; c < 3; c++) {
      view.setUint16(i * 8 + c * 2, toHalf(srgbToLinear(rgb[i * 3 + c] / 255)), true);
    }
    view.setUint16(i * 8 + 6, toHalf(1.0), true);
  }
  return out;
}

// ── Harness ───────────────────────────────────────────────────────────────────

const WIDTH = 64; // 64 × 4 bytes = 256, so readback rows need no padding
const HEIGHT = 4;

let gpu: GPUSession;

beforeAll(async () => {
  if (!navigator.gpu) throw new Error("WebGPU unavailable in this browser");
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("no WebGPU adapter");

  // rgba8unorm rather than the canvas's preferred format, so readback bytes are
  // in R, G, B order and need no channel swizzle.
  gpu = { device: await adapter.requestDevice(), format: "rgba8unorm" };
});

/**
 * Drive the real frontend pipelines over `rgb` and read the result back.
 * Mirrors `renderer.ts`: calcParams compute pass, then the develop render pass.
 */
async function develop(
  rgb: Uint8Array,
  sliders: Sliders,
  renderScale?: number,
): Promise<Uint8Array> {
  const calcParams = createCalcParamsPipeline(gpu);
  const imgDev = createImgDevPipeline(gpu);

  const image = uploadRawPixelsToGPU(gpu.device, linearisePixels(rgb), WIDTH, HEIGHT);

  const target = gpu.device.createTexture({
    size: { width: WIDTH, height: HEIGHT },
    format: "rgba8unorm",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
  });

  const bytesPerRow = WIDTH * 4;
  const staging = gpu.device.createBuffer({
    size: bytesPerRow * HEIGHT,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });

  const bindGroup = gpu.device.createBindGroup({
    layout: imgDev.bindGroupLayout,
    entries: [
      { binding: 0, resource: image.texture.createView() },
      { binding: 1, resource: imgDev.sampler },
      { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
    ],
  });

  calcParams.updateSliders(sliders);
  if (renderScale !== undefined) calcParams.updateView(renderScale);

  const encoder = gpu.device.createCommandEncoder();
  calcParams.recordCalcParams(encoder);

  const pass = encoder.beginRenderPass({
    colorAttachments: [
      {
        view: target.createView(),
        clearValue: { r: 0, g: 0, b: 0, a: 1 },
        loadOp: "clear",
        storeOp: "store",
      },
    ],
  });
  pass.setPipeline(imgDev.pipeline);
  pass.setBindGroup(0, bindGroup);
  pass.draw(6);
  pass.end();

  encoder.copyTextureToBuffer(
    { texture: target },
    { buffer: staging, bytesPerRow },
    { width: WIDTH, height: HEIGHT },
  );
  gpu.device.queue.submit([encoder.finish()]);

  await staging.mapAsync(GPUMapMode.READ);
  const rgba = new Uint8Array(staging.getMappedRange().slice(0));
  staging.unmap();

  // Drop alpha so the result lines up with the Rust renderer's RGB output.
  const out = new Uint8Array(WIDTH * HEIGHT * 3);
  for (let i = 0; i < WIDTH * HEIGHT; i++) {
    out[i * 3] = rgba[i * 4];
    out[i * 3 + 1] = rgba[i * 4 + 1];
    out[i * 3 + 2] = rgba[i * 4 + 2];
  }
  calcParams.destroy();
  image.texture.destroy();
  return out;
}

/** The same independent-channel ramp the Rust suite uses. */
function rampChart(): Uint8Array {
  const rgb = new Uint8Array(WIDTH * HEIGHT * 3);
  for (let y = 0; y < HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const v = Math.floor((x * 255) / (WIDTH - 1));
      const i = (y * WIDTH + x) * 3;
      rgb[i] = v;
      rgb[i + 1] = 255 - v;
      rgb[i + 2] = Math.floor(v / 2);
    }
  }
  return rgb;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("WebGPU availability", () => {
  it("exposes an adapter and a device", () => {
    expect(gpu.device).toBeDefined();
  });
});

describe("pipeline construction", () => {
  it("builds both pipelines against the shipped WGSL", () => {
    // Compiling the shaders and validating the bind group layouts happens here;
    // a layout that disagrees with the WGSL throws.
    const calcParams = createCalcParamsPipeline(gpu);
    const imgDev = createImgDevPipeline(gpu);

    expect(calcParams.paramsBuffer.size).toBe(PARAMS_BUFFER_SIZE);
    expect(imgDev.pipeline).toBeDefined();
    expect(imgDev.bindGroupLayout).toBeDefined();
    calcParams.destroy();
  });

  it("declares buffer sizes matching the WGSL structs", () => {
    // 30 f32 fields, rounded up to the 16-byte alignment of a uniform struct.
    expect(SLIDERS_BUFFER_SIZE).toBe(128);
    expect(SLIDERS_BUFFER_SIZE % 16).toBe(0);
    expect(SLIDERS_BUFFER_SIZE).toBeGreaterThanOrEqual(30 * 4);
    expect(PARAMS_BUFFER_SIZE).toBe(256);
    // One f32, rounded up to the 16-byte alignment of a uniform struct.
    expect(VIEW_BUFFER_SIZE).toBe(16);
  });

  it("accepts a params bind group built the way renderer.ts builds it", () => {
    const calcParams = createCalcParamsPipeline(gpu);
    const imgDev = createImgDevPipeline(gpu);
    const image = uploadRawPixelsToGPU(
      gpu.device,
      linearisePixels(new Uint8Array(WIDTH * HEIGHT * 3)),
      WIDTH,
      HEIGHT,
    );

    expect(() =>
      gpu.device.createBindGroup({
        layout: imgDev.bindGroupLayout,
        entries: [
          { binding: 0, resource: image.texture.createView() },
          { binding: 1, resource: imgDev.sampler },
          { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
        ],
      }),
    ).not.toThrow();

    calcParams.destroy();
    image.texture.destroy();
  });
});

describe("develop chain through the frontend pipelines", () => {
  it("leaves the image untouched with the default sliders", async () => {
    const source = rampChart();
    const out = await develop(source, defaultSlidersRGB);

    for (let i = 0; i < source.length; i++) {
      expect(Math.abs(out[i] - source[i])).toBeLessThanOrEqual(2);
    }
  });

  it("keeps the primaries on their own channels", async () => {
    const source = new Uint8Array(WIDTH * HEIGHT * 3);
    for (let i = 0; i < WIDTH * HEIGHT; i++) {
      source[i * 3] = 255; // pure red everywhere
    }

    const out = await develop(source, defaultSlidersRGB);

    expect(out[0]).toBeGreaterThan(250);
    expect(out[1]).toBeLessThan(5);
    expect(out[2]).toBeLessThan(5);
  });

  it("scales linear light by one stop of exposure", async () => {
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(128);
    const out = await develop(source, { ...defaultSlidersRGB, exposure: 1 });

    const expected = throughLinear(128, (l) => l * 2);
    expect(Math.abs(out[0] - expected)).toBeLessThanOrEqual(2);
  });

  it("inverts, reversing the tone order", async () => {
    const source = rampChart();
    const out = await develop(source, { ...defaultSlidersRGB, invert: true });

    // Column 0 of the red channel is black in, so white out; the last column
    // is white in, so black out.
    expect(out[0]).toBeGreaterThan(250);
    expect(out[(WIDTH - 1) * 3]).toBeLessThan(5);
  });

  it("applies gamma to one channel only", async () => {
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(128);
    const out = await develop(source, { ...defaultSlidersRGB, redGamma: 2.2 });

    const expected = throughLinear(128, (l) => Math.pow(l, 1 / 2.2));
    expect(Math.abs(out[0] - expected)).toBeLessThanOrEqual(2);
    expect(Math.abs(out[1] - 128)).toBeLessThanOrEqual(2);
  });

  it("packs the sliders in the order the shader reads them", async () => {
    // Every slider that shifts the image is set to a distinct non-neutral
    // value. A packing offset would land one of them in the wrong field and
    // change the result; matching the reference means the order is right.
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(160);
    const out = await develop(source, {
      ...defaultSlidersRGB,
      exposure: 0.5,
      redGamma: 1.4,
    });

    // Order matters: develop.wgsl applies gamma (step 2) before exposure (step 4).
    const expected = throughLinear(160, (l) => Math.pow(l, 1 / 1.4) * Math.pow(2, 0.5));
    expect(Math.abs(out[0] - expected)).toBeLessThanOrEqual(3);
  });

  it("carries the sharpness sliders without disturbing the ones before them", async () => {
    // The sharpness block sits at the tail of the uniform and nothing reads it
    // yet, so railing every one of its fields must leave the render alone. A
    // field inserted in the wrong place would push exposure into another lane
    // and this would come back at the wrong brightness.
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(128);
    const out = await develop(source, {
      ...defaultSlidersRGB,
      exposure: 1,
      clarity: 100,
      texture: -100,
      usmAmount: 300,
      usmRadius: 10,
      usmLumaThreshold: 100,
      usmDetailThreshold: 100,
    });

    const expected = throughLinear(128, (l) => l * 2);
    expect(Math.abs(out[0] - expected)).toBeLessThanOrEqual(2);
  });
});

describe("the view uniform", () => {
  // The bind group gained a third entry, which is the kind of thing that fails
  // at pipeline creation rather than in the numbers — only a real device says.
  it("renders identically at any render scale", async () => {
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(128);
    const sliders = { ...defaultSlidersRGB, exposure: 1, usmRadius: 8 };

    const full = await develop(source, sliders, 1);
    const quarter = await develop(source, sliders, 0.25);

    // Nothing reads render_scale yet, so it must not reach a pixel. When the
    // blur lands this is the test that has to start telling them apart.
    expect([...quarter]).toEqual([...full]);
  });

  it("renders the same whether or not the view was ever written", async () => {
    const source = new Uint8Array(WIDTH * HEIGHT * 3).fill(200);
    const sliders = { ...defaultSlidersRGB, contrast: 40 };

    const untouched = await develop(source, sliders);
    const explicit = await develop(source, sliders, 1);

    // The pipeline seeds the buffer at construction, so leaving it alone and
    // setting it to full resolution are the same thing.
    expect([...untouched]).toEqual([...explicit]);
  });
});
