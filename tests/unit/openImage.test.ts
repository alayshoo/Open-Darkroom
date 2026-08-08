// tests/unit/openImage.test.ts
//
// The other half of the payload contract. `build_payload` in Rust writes these
// bytes and `image_prep.rs` checks that side; this checks the reader agrees.
// The buffer below is assembled to the same layout by hand, so a change to the
// framing has to break one side or the other.

import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const { openImage } = await import("$lib/utils/openImage");

const HIST_BINS = 256;

/** Build a payload exactly as `build_payload` does in image_opening.rs. */
function payload(
  width: number,
  height: number,
  fillPixel: (i: number) => number,
  histSeed = 0,
): ArrayBuffer {
  const pixelBytes = width * height * 8;
  const buffer = new ArrayBuffer(8 + pixelBytes + 3 * HIST_BINS * 4);
  const view = new DataView(buffer);

  view.setUint32(0, width, true);
  view.setUint32(4, height, true);

  const pixels = new Uint8Array(buffer, 8, pixelBytes);
  for (let i = 0; i < pixelBytes; i++) pixels[i] = fillPixel(i);

  const histOffset = 8 + pixelBytes;
  for (let channel = 0; channel < 3; channel++) {
    for (let bin = 0; bin < HIST_BINS; bin++) {
      const at = histOffset + (channel * HIST_BINS + bin) * 4;
      view.setUint32(at, histSeed + channel * 1000 + bin, true);
    }
  }

  return buffer;
}

describe("openImage", () => {
  beforeEach(() => invoke.mockReset());

  it("reads the dimensions from the header", async () => {
    invoke.mockResolvedValue(payload(9, 5, () => 0));

    const image = await openImage();

    expect(invoke).toHaveBeenCalledWith("open_image_file");
    expect(image.width).toBe(9);
    expect(image.height).toBe(5);
  });

  it("slices the pixels at eight bytes per pixel", async () => {
    invoke.mockResolvedValue(payload(4, 3, (i) => i % 256));

    const image = await openImage();

    expect(image.pixels).toHaveLength(4 * 3 * 8);
    expect(image.pixels[0]).toBe(0);
    expect(image.pixels[1]).toBe(1);
    expect(image.pixels[95]).toBe(95);
  });

  it("reads the three histograms in R, G, B order after the pixels", async () => {
    invoke.mockResolvedValue(payload(2, 2, () => 0));

    const image = await openImage();

    expect(image.histR).toHaveLength(HIST_BINS);
    expect(image.histG).toHaveLength(HIST_BINS);
    expect(image.histB).toHaveLength(HIST_BINS);

    // The generator writes channel * 1000 + bin.
    expect(image.histR[0]).toBe(0);
    expect(image.histR[255]).toBe(255);
    expect(image.histG[0]).toBe(1000);
    expect(image.histB[0]).toBe(2000);
    expect(image.histB[255]).toBe(2255);
  });

  it("does not let the pixel and histogram regions overlap", async () => {
    // A one-pixel image makes an off-by-one in the offset obvious: the first
    // histogram value would otherwise be read out of the pixel block.
    invoke.mockResolvedValue(payload(1, 1, () => 0xff, 7));

    const image = await openImage();

    expect(image.pixels).toHaveLength(8);
    expect([...image.pixels]).toEqual(Array(8).fill(0xff));
    expect(image.histR[0]).toBe(7);
  });

  it("handles a preview whose dimensions are not powers of two", async () => {
    invoke.mockResolvedValue(payload(2048, 683, () => 1));

    const image = await openImage();

    expect(image.width).toBe(2048);
    expect(image.height).toBe(683);
    expect(image.pixels).toHaveLength(2048 * 683 * 8);
    expect(image.histR).toHaveLength(HIST_BINS);
  });
});
