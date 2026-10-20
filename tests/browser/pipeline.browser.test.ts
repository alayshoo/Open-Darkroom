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

import { SHARP_FORMAT, WORKING_FORMAT, type GPUSession } from "$lib/types/gpuTypes";
import { defaultSlidersRGB, type Sliders } from "$lib/types/imgParameters";
import { CURVE_CHANNELS } from "$lib/types/curves";
import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
import { parseImagePayload } from "$lib/utils/openImage";
import {
  createCompositeStage,
  createDevelopStage,
  createEncodeStage,
  createSharpHorizontalStage,
  createSharpVerticalStage,
  createSourceSampler,
} from "$lib/gpu/pipelines/chainStages";
import {
  createStageBindGroup,
  recordFullScreenPass,
} from "$lib/gpu/pipelines/renderStagePipeline";
import {
  createCalcParamsPipeline,
  PARAMS_BUFFER_SIZE,
  SHARP_BUFFER_SIZE,
  SLIDERS_BUFFER_SIZE,
  VIEW_BUFFER_SIZE,
} from "$lib/gpu/pipelines/calcParamsPipeline";
import { sharpnessIsVisible } from "$lib/gpu/sharpnessVisibility";
import { createHistogramPipeline } from "$lib/gpu/pipelines/histogramPipeline";
import { createOutputSpaceBuffer } from "$lib/gpu/outputSpaceBuffer";
import { IDENTITY, SRGB_TO_WORKING } from "$lib/gpu/workingSpace";
import {
  OUTPUT_SPACE_IDS,
  type OutputSpaceId,
} from "$lib/gpu/outputSpace";
import { OUTPUT_SPACE_BUFFER_SIZE } from "$lib/gpu/outputSpaceBuffer";

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

// Dimensions of the image the charts stand in for. Only clarity's radius reads
// these, and only to take a fraction of the short edge.
const FULL_WIDTH = 4000;
const FULL_HEIGHT = 3000;

let gpu: GPUSession;

beforeAll(async () => {
  if (!navigator.gpu) throw new Error("WebGPU unavailable in this browser");
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) throw new Error("no WebGPU adapter");

  // rgba8unorm rather than the canvas's preferred format, so readback bytes are
  // in R, G, B order and need no channel swizzle.
  // The suite reads pixels back rather than timing anything, so it takes the
  // device without the optional timestamp feature the app asks for.
  gpu = {
    device: await adapter.requestDevice(),
    format: "rgba8unorm",
    // sRGB rather than whatever the test machine's display can show: these
    // tests compare bytes against the Rust renderer, which is given the same
    // destination.
    displaySpace: "srgb",
    canTimestamp: false,
    adapterInfo: null,
    adapterFeatures: [],
  };
});

/** Overrides for a run over something other than the default chart. */
interface DevelopOptions {
  width?: number;
  height?: number;
  /** RGBA f16 pixels to upload as they stand, rather than linearising `rgb`. */
  pixels?: Uint8Array;
  /** Where to encode for. Defaults to sRGB, unproofed, as the Rust suite does. */
  output?: OutputSpaceId;
  /** The export space to proof against, if any. */
  proof?: OutputSpaceId | null;
  /** Treat the source as already in the working space, so only the output
   *  transform runs — the one way to reach colour outside the destination. */
  alreadyWorking?: boolean;
}

/**
 * Drive the real frontend pipelines over `rgb` and read the result back.
 * Mirrors `renderer.ts`: the calcParams compute pass, then the three render
 * passes of the develop chain — develop, composite, output transform.
 */
async function develop(
  rgb: Uint8Array,
  sliders: Sliders,
  renderScale?: number,
  options: DevelopOptions = {},
): Promise<Uint8Array> {
  const width = options.width ?? WIDTH;
  const height = options.height ?? HEIGHT;

  const calcParams = createCalcParamsPipeline(gpu);
  const develop = createDevelopStage(gpu);
  const sharpH = createSharpHorizontalStage(gpu);
  const sharpV = createSharpVerticalStage(gpu);
  const composite = createCompositeStage(gpu);
  const colorSpaceEncode = createEncodeStage(gpu);
  const outputSpace = createOutputSpaceBuffer(gpu);
  outputSpace.set(options.output ?? "srgb", options.proof ?? null);
  const sampler = createSourceSampler(gpu);

  const image = uploadRawPixelsToGPU(
    gpu.device,
    options.pixels ?? linearisePixels(rgb),
    width,
    height,
  );

  const intermediate = (label: string, format: GPUTextureFormat) =>
    gpu.device.createTexture({
      label,
      size: { width, height },
      format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
    });
  const developTexture = intermediate("developed", WORKING_FORMAT);
  const sharpHTexture = intermediate("sharp h", SHARP_FORMAT);
  const detailTexture = intermediate("detail", SHARP_FORMAT);
  const compositeTexture = intermediate("composited", WORKING_FORMAT);

  const target = gpu.device.createTexture({
    size: { width, height },
    format: "rgba8unorm",
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
  });

  // Readback rows are padded to the 256-byte alignment the copy requires, so a
  // width whose rows do not land on it is a case the harness handles rather
  // than a width the tests have to avoid.
  const bytesPerRow = Math.ceil((width * 4) / 256) * 256;
  const staging = gpu.device.createBuffer({
    size: bytesPerRow * height,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });

  const bindGroup = createStageBindGroup(gpu, develop, [
    { binding: 0, resource: image.texture.createView() },
    { binding: 1, resource: sampler },
    { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
  ]);

  calcParams.updateSliders(sliders);
  calcParams.updateSourceSpace(options.alreadyWorking ? IDENTITY : SRGB_TO_WORKING);
  if (renderScale !== undefined) calcParams.updateView(renderScale, FULL_WIDTH, FULL_HEIGHT);

  const encoder = gpu.device.createCommandEncoder();
  calcParams.recordCalcParams(encoder);

  recordFullScreenPass(encoder, develop, developTexture.createView(), bindGroup);

  if (sharpnessIsVisible(sliders, renderScale ?? 1)) {
    recordFullScreenPass(
      encoder,
      sharpH,
      sharpHTexture.createView(),
      createStageBindGroup(gpu, sharpH, [
        { binding: 0, resource: developTexture.createView() },
        { binding: 1, resource: developTexture.createView() },
        { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
      ]),
    );
    recordFullScreenPass(
      encoder,
      sharpV,
      detailTexture.createView(),
      createStageBindGroup(gpu, sharpV, [
        { binding: 0, resource: sharpHTexture.createView() },
        { binding: 1, resource: developTexture.createView() },
        { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
      ]),
    );
  }

  recordFullScreenPass(
    encoder,
    composite,
    compositeTexture.createView(),
    createStageBindGroup(gpu, composite, [
      { binding: 0, resource: developTexture.createView() },
      { binding: 1, resource: detailTexture.createView() },
      { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
    ]),
  );
  recordFullScreenPass(
    encoder,
    colorSpaceEncode,
    target.createView(),
    createStageBindGroup(gpu, colorSpaceEncode, [
      { binding: 0, resource: compositeTexture.createView() },
      { binding: 1, resource: { buffer: outputSpace.buffer } },
    ]),
  );

  encoder.copyTextureToBuffer(
    { texture: target },
    { buffer: staging, bytesPerRow },
    { width, height },
  );
  gpu.device.queue.submit([encoder.finish()]);

  await staging.mapAsync(GPUMapMode.READ);
  const rgba = new Uint8Array(staging.getMappedRange().slice(0));
  staging.unmap();

  // Drop alpha and the row padding so the result lines up with the Rust
  // renderer's RGB output.
  const out = new Uint8Array(width * height * 3);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = y * bytesPerRow + x * 4;
      const to = (y * width + x) * 3;
      out[to] = rgba[from];
      out[to + 1] = rgba[from + 1];
      out[to + 2] = rgba[from + 2];
    }
  }
  calcParams.destroy();
  outputSpace.destroy();
  image.texture.destroy();
  developTexture.destroy();
  sharpHTexture.destroy();
  detailTexture.destroy();
  compositeTexture.destroy();
  return out;
}

/**
 * Run the histogram compute pipeline over `rgb`.
 *
 * The image is 64 × 4 against a 16 × 16 workgroup, so most of the last
 * workgroup's rows fall outside it — which is the case the shader has to gate
 * rather than return out of, since every invocation must reach its barriers.
 */
async function histogram(
  rgb: Uint8Array,
  sliders: Sliders,
): Promise<{ r: Uint32Array; g: Uint32Array; b: Uint32Array; scale: number }> {
  const pipeline = createHistogramPipeline(gpu);
  const image = uploadRawPixelsToGPU(gpu.device, linearisePixels(rgb), WIDTH, HEIGHT);
  pipeline.setSourceSpace(SRGB_TO_WORKING);
  try {
    // No target: the app draws the curves from the bins on the GPU, but what is
    // under test here is the binning, so this stops at the buffer.
    pipeline.render(image.texture, sliders);
    return await pipeline.readBins();
  } finally {
    image.texture.destroy();
    pipeline.destroy();
  }
}

/** Total weight one pixel casts per channel. Mirrors VOTE in histogram.wgsl. */
const VOTE = 256;
const TOTAL_WEIGHT = WIDTH * HEIGHT * VOTE;

function sum(bins: Uint32Array): number {
  return bins.reduce((total, v) => total + v, 0);
}

/** Weighted mean bin index — where a channel's mass sits. */
function centroid(bins: Uint32Array): number {
  let weighted = 0;
  for (let i = 0; i < bins.length; i++) weighted += i * bins[i];
  return weighted / sum(bins);
}

/** A solid colour across the whole chart. */
function solid(r: number, g: number, b: number): Uint8Array {
  const rgb = new Uint8Array(WIDTH * HEIGHT * 3);
  for (let i = 0; i < WIDTH * HEIGHT; i++) {
    rgb[i * 3] = r;
    rgb[i * 3 + 1] = g;
    rgb[i * 3 + 2] = b;
  }
  return rgb;
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
  it("builds every stage of the chain against the shipped WGSL", () => {
    // Compiling the shaders and validating the bind group layouts happens here;
    // a layout that disagrees with the WGSL throws.
    const calcParams = createCalcParamsPipeline(gpu);
    const stages = [
      createDevelopStage(gpu),
      createSharpHorizontalStage(gpu),
      createSharpVerticalStage(gpu),
      createCompositeStage(gpu),
      createEncodeStage(gpu),
    ];

    expect(calcParams.paramsBuffer.size).toBe(PARAMS_BUFFER_SIZE);
    for (const stage of stages) {
      expect(stage.pipeline, stage.label).toBeDefined();
      expect(stage.bindGroupLayout, stage.label).toBeDefined();
    }
    calcParams.destroy();
  });

  it("declares buffer sizes matching the WGSL structs", () => {
    // 54 f32 fields, rounded up to the 16-byte alignment of a uniform struct.
    expect(SLIDERS_BUFFER_SIZE).toBe(224);
    expect(SLIDERS_BUFFER_SIZE % 16).toBe(0);
    expect(SLIDERS_BUFFER_SIZE).toBeGreaterThanOrEqual(54 * 4);
    expect(PARAMS_BUFFER_SIZE).toBe(4496);
    // One f32, rounded up to the 16-byte alignment of a uniform struct.
    expect(VIEW_BUFFER_SIZE).toBe(16);
    // Twelve f32 — three bands' amounts and sigmas, two thresholds, padding.
    expect(SHARP_BUFFER_SIZE).toBe(48);
  });

  it("accepts a params bind group built the way renderer.ts builds it", () => {
    const calcParams = createCalcParamsPipeline(gpu);
    const develop = createDevelopStage(gpu);
    const sampler = createSourceSampler(gpu);
    const image = uploadRawPixelsToGPU(
      gpu.device,
      linearisePixels(new Uint8Array(WIDTH * HEIGHT * 3)),
      WIDTH,
      HEIGHT,
    );

    expect(() =>
      createStageBindGroup(gpu, develop, [
        { binding: 0, resource: image.texture.createView() },
        { binding: 1, resource: sampler },
        { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
      ]),
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

describe("the sharpness stage", () => {
  /** A vertical edge, dark on the left and bright on the right. */
  function edgeChart(): Uint8Array {
    const rgb = new Uint8Array(WIDTH * HEIGHT * 3);
    for (let y = 0; y < HEIGHT; y++) {
      for (let x = 0; x < WIDTH; x++) {
        const v = x < WIDTH / 2 ? 60 : 190;
        const i = (y * WIDTH + x) * 3;
        rgb[i] = v;
        rgb[i + 1] = v;
        rgb[i + 2] = v;
      }
    }
    return rgb;
  }

  it("overshoots on both sides of an edge", async () => {
    const source = edgeChart();
    const plain = await develop(source, defaultSlidersRGB);
    const sharp = await develop(source, {
      ...defaultSlidersRGB,
      usmAmount: 150,
      usmRadius: 2,
    });

    const mid = WIDTH / 2;
    expect(sharp[(mid - 1) * 3]).toBeLessThan(plain[(mid - 1) * 3]);
    expect(sharp[mid * 3]).toBeGreaterThan(plain[mid * 3]);
    // The flats, far from the edge, have nothing to sharpen.
    expect(sharp[2 * 3]).toBe(plain[2 * 3]);
  });

  it("leaves the image alone when the amount is zero", async () => {
    const source = edgeChart();
    const plain = await develop(source, defaultSlidersRGB);
    const idle = await develop(source, {
      ...defaultSlidersRGB,
      usmAmount: 0,
      usmRadius: 9,
      usmLumaThreshold: 60,
    });

    expect(Array.from(idle)).toEqual(Array.from(plain));
  });

  it("fades the mask out as the render scale falls", async () => {
    const source = edgeChart();
    const plain = await develop(source, defaultSlidersRGB);
    const sliders = { ...defaultSlidersRGB, usmAmount: 200, usmRadius: 1 };

    const atFull = await develop(source, sliders, 1);
    const zoomedOut = await develop(source, sliders, 0.05);

    const mid = WIDTH / 2;
    expect(atFull[mid * 3]).toBeGreaterThan(plain[mid * 3]);
    expect(Array.from(zoomedOut)).toEqual(Array.from(plain));
  });

  it("moves the image with texture and with clarity", async () => {
    // Both bands live in the same detail texture, so this catches a channel
    // wired to the wrong slot on the TypeScript side.
    const source = edgeChart();
    const plain = await develop(source, defaultSlidersRGB);

    const textured = await develop(source, { ...defaultSlidersRGB, texture: 100 });
    const clarified = await develop(source, { ...defaultSlidersRGB, clarity: 100 });

    expect(Array.from(textured)).not.toEqual(Array.from(plain));
    expect(Array.from(clarified)).not.toEqual(Array.from(plain));
    expect(Array.from(textured)).not.toEqual(Array.from(clarified));
  });

  it("agrees with the renderer about when the blur can be skipped", () => {
    // The predicate has to be false wherever calcParams fades the amount to
    // zero, or the renderer would skip a pass that still had work to do.
    expect(sharpnessIsVisible(defaultSlidersRGB, 1)).toBe(false);
    expect(sharpnessIsVisible({ ...defaultSlidersRGB, usmAmount: 100 }, 1)).toBe(true);
    expect(sharpnessIsVisible({ ...defaultSlidersRGB, usmAmount: 100 }, 0.05)).toBe(false);

    // Texture's scale is fixed and fine, so it leaves a zoomed-out view early.
    expect(sharpnessIsVisible({ ...defaultSlidersRGB, texture: 100 }, 1)).toBe(true);
    expect(sharpnessIsVisible({ ...defaultSlidersRGB, texture: 100 }, 0.1)).toBe(false);

    // Clarity's is tens of pixels, so it survives any preview scale in practice.
    expect(sharpnessIsVisible({ ...defaultSlidersRGB, clarity: 100 }, 0.1)).toBe(true);
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

describe("histogram compute", () => {
  // Each pixel splits one VOTE between two adjacent bins, so the weight in a
  // channel is fixed no matter what the tone chain does to the values. That
  // makes it the one assertion that holds without assuming anything about the
  // develop maths — and votes lost in the workgroup merge, or counted twice by
  // the out-of-bounds invocations, show up here and nowhere else.
  it("conserves one vote per pixel per channel", async () => {
    const bins = await histogram(rampChart(), defaultSlidersRGB);

    expect(sum(bins.r)).toBe(TOTAL_WEIGHT);
    expect(sum(bins.g)).toBe(TOTAL_WEIGHT);
    expect(sum(bins.b)).toBe(TOTAL_WEIGHT);
  });

  it("keeps the three channels apart", async () => {
    // Ascending red, descending green, and blue at half the red value. The
    // three distributions are distinct, so a channel reading another's slice of
    // the shared bin buffer cannot pass this.
    const bins = await histogram(rampChart(), defaultSlidersRGB);

    expect(centroid(bins.b)).toBeLessThan(centroid(bins.r));
    expect(centroid(bins.b)).toBeLessThan(centroid(bins.g));
    expect([...bins.r]).not.toEqual([...bins.g]);
  });

  it("puts a solid colour in one place per channel", async () => {
    const bins = await histogram(solid(32, 128, 220), defaultSlidersRGB);

    // Every pixel carries the same value, so its vote splits between the same
    // two adjacent bins — nothing should land anywhere else.
    for (const channel of [bins.r, bins.g, bins.b]) {
      const occupied = [...channel.entries()].filter(([, weight]) => weight > 0);

      expect(occupied.length).toBeGreaterThanOrEqual(1);
      expect(occupied.length).toBeLessThanOrEqual(2);
      if (occupied.length === 2) {
        expect(occupied[1][0] - occupied[0][0]).toBe(1);
      }
      expect(sum(channel)).toBe(TOTAL_WEIGHT);
    }

    // Darkest channel lowest, brightest highest.
    expect(centroid(bins.r)).toBeLessThan(centroid(bins.g));
    expect(centroid(bins.g)).toBeLessThan(centroid(bins.b));
  });

  // The histogram runs its own copy of the develop chain, so it can agree with
  // the shader it was copied from and still describe a different image than the
  // one on screen. It did: it binned the working space while the preview showed
  // the destination, and the two disagreed by twenty code values on saturated
  // content without either looking wrong.
  //
  // Both now end in the shared output transform, which is what this checks
  // numerically rather than by looking for the chunk's text. Saturated colours
  // are the point: a neutral lands in the same bin either way.
  it("bins the same values the preview renders", async () => {
    const probes: [number, number, number][] = [
      [220, 40, 40],
      [40, 200, 90],
      [60, 70, 210],
      [200, 180, 40],
    ];

    const sliders: Sliders = { ...defaultSlidersRGB, saturation: 25, vibrance: 40 };

    for (const [r, g, b] of probes) {
      const rgb = solid(r, g, b);
      const rendered = await develop(rgb, sliders);
      const bins = await histogram(rgb, sliders);

      // A solid colour splits one vote between two adjacent bins, so the
      // weighted centre of each channel is where the preview's value landed.
      const channels: [string, Uint32Array, number][] = [
        ["red", bins.r, rendered[0]],
        ["green", bins.g, rendered[1]],
        ["blue", bins.b, rendered[2]],
      ];

      for (const [name, channel, value] of channels) {
        expect(sum(channel), `${name} lost votes on ${[r, g, b]}`).toBe(TOTAL_WEIGHT);
        expect(
          centroid(channel),
          `${name} on ${[r, g, b]}: the preview rendered ${value}, the bins centre on ` +
            `${centroid(channel).toFixed(2)} — the two are measuring different images`,
        ).toBeCloseTo(value, 0);
      }
    }
  });
});

describe("histogram draw", () => {
  // A pipeline that fails validation does not throw: the error surfaces on the
  // device, and the submit carrying it drops the whole frame, bins included.
  it("draws the main histogram and every curve backdrop without a validation error", async () => {
    gpu.device.pushErrorScope("validation");
    const pipeline = createHistogramPipeline(gpu);
    const image = uploadRawPixelsToGPU(gpu.device, linearisePixels(rampChart()), WIDTH, HEIGHT);
    const target = gpu.device.createTexture({
      size: { width: 32, height: 16 },
      format: gpu.format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT,
    });
    pipeline.setSourceSpace(SRGB_TO_WORKING);
    for (const channel of CURVE_CHANNELS) {
      pipeline.render(image.texture, defaultSlidersRGB, target.createView(), {
        view: target.createView(),
        channel,
      });
    }
    const error = await gpu.device.popErrorScope();
    target.destroy();
    image.texture.destroy();
    pipeline.destroy();
    expect(error?.message ?? null).toBeNull();
  });
});

describe("histogram scale reduction", () => {
  // The value the draw normalises against is reduced on the GPU across all
  // three channels at once, through a workgroup tree that has to survive its
  // barriers. A plain scan of the same bins says what it should have found.
  function tallestBin(bins: {
    r: Uint32Array;
    g: Uint32Array;
    b: Uint32Array;
  }): number {
    let best = 0;
    for (const channel of [bins.r, bins.g, bins.b]) {
      // Bins 0 and 255 are excluded: a clipped region spikes there far above
      // everything else, and normalising to it flattens the rest.
      for (let i = 1; i < 255; i++) best = Math.max(best, channel[i]);
    }
    return Math.max(best, 1);
  }

  it("finds the tallest bin across the three channels", async () => {
    const bins = await histogram(rampChart(), defaultSlidersRGB);
    expect(bins.scale).toBe(tallestBin(bins));
  });

  it("ignores the clipped-black bin", async () => {
    // Half the chart pure black, the rest spread thinly across the midtones.
    // The spike in bin 0 is two orders of magnitude above anything in the
    // interior, so a reduction that counted it would flatten the rest of the
    // curve onto the baseline.
    const pixels = WIDTH * HEIGHT;
    const rgb = new Uint8Array(pixels * 3);
    for (let i = pixels / 2; i < pixels; i++) {
      const v = 64 + ((i - pixels / 2) % 128);
      rgb[i * 3] = v;
      rgb[i * 3 + 1] = v;
      rgb[i * 3 + 2] = v;
    }
    const bins = await histogram(rgb, defaultSlidersRGB);

    expect(bins.scale).toBe(tallestBin(bins));
    // The spike was genuinely the tallest bin, and genuinely left out.
    expect(bins.r[0]).toBeGreaterThan(bins.scale);
  });

  it("never reports zero, so the draw cannot divide by it", async () => {
    const bins = await histogram(solid(0, 0, 0), defaultSlidersRGB);
    expect(bins.scale).toBeGreaterThan(0);
  });
});

// ── Image integrity ───────────────────────────────────────────────────────────
//
// The tests above ask what the chain does to a value. These ask whether the
// pixel that came out is the pixel that went in, at the position it went in at
// — the class of fault where the maths is right and the geometry is not: a roll,
// a flip, a row stride off by a padding, a buffer read from the wrong offset.
//
// They start from a payload framed the way the backend frames one, because the
// pixels reaching the GPU in the app are a view into the middle of that payload
// rather than a buffer of their own, and an upload that reads from the start of
// the buffer instead shifts the whole image by the header.

/** Awkward on purpose: rows land off the 256-byte readback alignment. */
const INTEGRITY_WIDTH = 253;
const INTEGRITY_HEIGHT = 61;

/**
 * A chart where every pixel is far from its neighbours, so a displacement of
 * even one pixel shows up as a large difference rather than a small one. A
 * gradient would hide a shift inside the tolerance the round trip needs.
 */
function scatterChart(width: number, height: number): Uint8Array {
  const rgb = new Uint8Array(width * height * 3);
  for (let i = 0; i < width * height; i++) {
    // Odd multipliers over a byte, so consecutive pixels land far apart and the
    // sequence does not repeat within a row or a column.
    rgb[i * 3] = (i * 149 + 11) & 0xff;
    rgb[i * 3 + 1] = (i * 83 + 197) & 0xff;
    rgb[i * 3 + 2] = (i * 211 + 61) & 0xff;
  }
  return rgb;
}

/**
 * Six u32s of dimensions, the source → working matrix, then the flags word.
 * Must match PAYLOAD_HEADER_BYTES in image_opening.rs.
 */
const HEADER = 64;

/** Frame pixels the way `build_payload` in image_opening.rs does. */
function backendPayload(rgb: Uint8Array, width: number, height: number): ArrayBuffer {
  const pixels = linearisePixels(rgb);
  // The overview is not read here, but it has to be present at its declared
  // size: it sits between the preview and the histograms.
  const overviewBytes = width * height * 8;
  const buffer = new ArrayBuffer(
    HEADER + pixels.length + overviewBytes + 3 * 256 * 4,
  );
  const view = new DataView(buffer);

  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  view.setUint32(8, width, true);
  view.setUint32(12, height, true);
  view.setUint32(16, width, true);
  view.setUint32(20, height, true);
  // An untagged file's matrix, which is what the backend sends for one.
  for (let i = 0; i < 9; i++) view.setFloat32(24 + i * 4, SRGB_TO_WORKING[i], true);
  view.setUint32(60, 0, true);
  new Uint8Array(buffer, HEADER, pixels.length).set(pixels);
  new Uint8Array(buffer, HEADER + pixels.length, overviewBytes).set(pixels);

  // Histograms are not read here, but they have to be present: they are what
  // makes the pixels an interior slice rather than the tail of the buffer.
  const histAt = HEADER + pixels.length + overviewBytes;
  for (let i = 0; i < 3 * 256; i++) {
    view.setUint32(histAt + i * 4, i, true);
  }
  return buffer;
}

/**
 * Where `out` sits relative to `source`, in whole pixels of the row-major
 * order, or null if no displacement lines them up. Reported on failure so a
 * geometry fault names itself instead of arriving as a wall of wrong bytes.
 */
function displacement(out: Uint8Array, source: Uint8Array, search = 8): number | null {
  for (let shift = -search; shift <= search; shift++) {
    if (shift === 0) continue;
    let worst = 0;
    // Skip the pixels that wrapped: what is being identified is the body.
    for (let i = Math.max(0, -shift) + search; i < source.length / 3 - search; i++) {
      const from = (i + shift) * 3;
      for (let c = 0; c < 3; c++) worst = Math.max(worst, Math.abs(out[from + c] - source[i * 3 + c]));
      if (worst > 2) break;
    }
    if (worst <= 2) return shift;
  }
  return null;
}

describe("image integrity", () => {
  it("returns the image it was given, pixel for pixel, at identity", async () => {
    const source = scatterChart(INTEGRITY_WIDTH, INTEGRITY_HEIGHT);
    const payload = parseImagePayload(
      backendPayload(source, INTEGRITY_WIDTH, INTEGRITY_HEIGHT),
    );

    expect(payload.width).toBe(INTEGRITY_WIDTH);
    expect(payload.height).toBe(INTEGRITY_HEIGHT);
    // The pixels are a window into the payload, not a buffer of their own.
    expect(payload.pixels.byteOffset).toBe(HEADER);

    const out = await develop(source, defaultSlidersRGB, 1, {
      width: payload.width,
      height: payload.height,
      pixels: payload.pixels,
    });

    let worst = 0;
    let worstAt = -1;
    for (let i = 0; i < source.length; i++) {
      const delta = Math.abs(out[i] - source[i]);
      if (delta > worst) {
        worst = delta;
        worstAt = i;
      }
    }

    if (worst > 2) {
      const shift = displacement(out, source);
      const pixel = Math.floor(worstAt / 3);
      throw new Error(
        shift === null
          ? `pixel ${pixel} (${pixel % INTEGRITY_WIDTH}, ${Math.floor(pixel / INTEGRITY_WIDTH)}) ` +
            `is off by ${worst}, and no whole-pixel displacement explains it`
          : `the rendered image is the source displaced by ${shift} pixel(s)`,
      );
    }
  });

  it("holds the edges in place", async () => {
    // The corners are where a displacement leaves its wrapped pixels, so they
    // get their own assertion: an error confined to a handful of pixels at one
    // edge is a fraction of the frame small enough for a whole-image tolerance
    // to swallow.
    const source = scatterChart(INTEGRITY_WIDTH, INTEGRITY_HEIGHT);
    const payload = parseImagePayload(
      backendPayload(source, INTEGRITY_WIDTH, INTEGRITY_HEIGHT),
    );

    const out = await develop(source, defaultSlidersRGB, 1, {
      width: payload.width,
      height: payload.height,
      pixels: payload.pixels,
    });

    const at = (x: number, y: number) => (y * INTEGRITY_WIDTH + x) * 3;
    const corners = [
      ["top left", at(0, 0)],
      ["top right", at(INTEGRITY_WIDTH - 1, 0)],
      ["bottom left", at(0, INTEGRITY_HEIGHT - 1)],
      ["bottom right", at(INTEGRITY_WIDTH - 1, INTEGRITY_HEIGHT - 1)],
    ] as const;

    for (const [name, i] of corners) {
      for (let c = 0; c < 3; c++) {
        expect(Math.abs(out[i + c] - source[i + c]), name).toBeLessThanOrEqual(2);
      }
    }

    // Both ends of every row, which is where a stride fault surfaces first.
    for (let y = 0; y < INTEGRITY_HEIGHT; y++) {
      for (const x of [0, INTEGRITY_WIDTH - 1]) {
        const i = at(x, y);
        for (let c = 0; c < 3; c++) {
          expect(Math.abs(out[i + c] - source[i + c]), `(${x}, ${y})`).toBeLessThanOrEqual(2);
        }
      }
    }
  });
});

describe("the output space", () => {
  // A saturated working-space primary, which is well outside sRGB and outside
  // Display P3. Reached by declaring the source already in the working space —
  // the usual sRGB source matrix and the output transform are inverses, so
  // nothing fed through it can land outside the destination.
  const WORKING_RED = solid(255, 0, 0);

  function firstPixel(out: Uint8Array): [number, number, number] {
    return [out[0], out[1], out[2]];
  }

  it("declares a buffer matching the WGSL struct", () => {
    // Three mat3x3 of three 16-byte columns, then two u32, bound 16-aligned.
    expect(OUTPUT_SPACE_BUFFER_SIZE).toBe(160);
    expect(OUTPUT_SPACE_BUFFER_SIZE % 16).toBe(0);
    expect(OUTPUT_SPACE_BUFFER_SIZE).toBeGreaterThanOrEqual(3 * 48 + 8);

    const buffer = createOutputSpaceBuffer(gpu);
    expect(buffer.buffer.size).toBe(OUTPUT_SPACE_BUFFER_SIZE);
    // Every space has to be writable without the padding assertions tripping.
    for (const id of OUTPUT_SPACE_IDS) buffer.set(id, "srgb");
    buffer.destroy();
  });

  // The matrix reaching the shader is the one this module derives. If the
  // uniform were never bound, packed without the column padding, or bound at
  // the wrong index, the encode stage would read zeroes and the frame would be
  // black — so a non-black render is itself the wiring assertion.
  it("reaches the shader, so each destination encodes differently", async () => {
    const results = new Map<OutputSpaceId, [number, number, number]>();
    for (const id of OUTPUT_SPACE_IDS) {
      const out = await develop(WORKING_RED, defaultSlidersRGB, undefined, {
        output: id,
        alreadyWorking: true,
      });
      const pixel = firstPixel(out);
      expect(pixel.some((c) => c > 0), `${id} rendered black`).toBe(true);
      results.set(id, pixel);
    }

    // Rec.2020 is the working space, so its own red needs no compression and
    // comes through as a pure primary. Every narrower space has to move it:
    // either the other channels rise, or the red itself comes down when one of
    // them reaches the floor first, which is what Adobe RGB's green does.
    expect(results.get("rec2020")).toEqual([255, 0, 0]);
    for (const id of ["srgb", "display-p3", "adobe-rgb"] as OutputSpaceId[]) {
      expect(results.get(id), `${id} left the primary untouched`).not.toEqual([255, 0, 0]);
    }

    // And all four differ from each other, so no two are reading the same
    // matrix. The exact values are the Rust suite's business; this is wiring.
    const distinct = new Set([...results.values()].map((p) => p.join(",")));
    expect(distinct.size).toBe(OUTPUT_SPACE_IDS.length);
  });

  // What soft-proofing is for: on a display wider than the export, the preview
  // shows colour the file cannot hold. Proofing squeezes the signal through the
  // export space's gamut and back, so the preview loses exactly what the file
  // will lose.
  it("squeezes the preview through the proofed gamut", async () => {
    const direct = firstPixel(
      await develop(WORKING_RED, defaultSlidersRGB, undefined, {
        output: "display-p3",
        alreadyWorking: true,
      }),
    );
    const proofed = firstPixel(
      await develop(WORKING_RED, defaultSlidersRGB, undefined, {
        output: "display-p3",
        proof: "srgb",
        alreadyWorking: true,
      }),
    );

    // Proofing the narrower space can only cost saturation, which on a red
    // means the other two channels come up.
    expect(proofed[1]).toBeGreaterThan(direct[1]);
    expect(proofed[2]).toBeGreaterThan(direct[2]);
  });

  // And it has to be a no-op on colour the proofed space can already hold,
  // or every proofed preview would be wrong about in-gamut colour too.
  it("leaves colour the proofed space can hold alone", async () => {
    const neutral = solid(128, 128, 128);
    const direct = firstPixel(
      await develop(neutral, defaultSlidersRGB, undefined, { output: "display-p3" }),
    );
    const proofed = firstPixel(
      await develop(neutral, defaultSlidersRGB, undefined, {
        output: "display-p3",
        proof: "srgb",
      }),
    );

    for (let c = 0; c < 3; c++) {
      expect(Math.abs(direct[c] - proofed[c]), `channel ${c}`).toBeLessThanOrEqual(1);
    }
  });

  // The histogram runs its own copy of the chain and measures after the same
  // transform, so a destination it was not told about would describe a
  // different picture from the one on screen.
  it("reaches the histogram as well as the preview", async () => {
    const pipeline = createHistogramPipeline(gpu);
    const image = uploadRawPixelsToGPU(gpu.device, linearisePixels(solid(200, 40, 40)), WIDTH, HEIGHT);
    try {
      const binsFor = async (output: OutputSpaceId) => {
        pipeline.setSourceSpace(SRGB_TO_WORKING);
        pipeline.setOutputSpace(output, null);
        pipeline.render(image.texture, defaultSlidersRGB);
        const { r } = await pipeline.readBins();
        // Where the red channel's mass sits, which is what the curve draws.
        return r.reduce((at, count, bin) => at + count * bin, 0) / r.reduce((a, b) => a + b, 0);
      };

      // Adobe RGB's curve has no linear toe and its gamut is wider, so the same
      // red cannot land in the same bin as it does in sRGB.
      expect(Math.abs((await binsFor("srgb")) - (await binsFor("adobe-rgb")))).toBeGreaterThan(1);
    } finally {
      image.texture.destroy();
      pipeline.destroy();
    }
  });
});
