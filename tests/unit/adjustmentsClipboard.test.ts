// tests/unit/adjustmentsClipboard.test.ts
//
// The document written by a copy is the one a paste has to read back, with a
// text editor in between. The reader is hand-rolled rather than a YAML library,
// so the round trip and the leniency it promises are pinned here.

import { describe, expect, it, vi } from "vitest";
import { defaultSlidersRGB, modeKeys, type Sliders } from "$lib/types/imgParameters";

const writeText = vi.hoisted(() => vi.fn());
const readText = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText, readText }));

const { copyAdjustments, parseAdjustments } = await import(
  "$lib/utils/adjustmentsClipboard"
);

const normalKeys = modeKeys(false);
const darkroomKeys = modeKeys(true);

/** The text a copy of `sliders` puts on the clipboard. */
async function copied(sliders: Sliders, keys: (keyof Sliders)[]) {
  writeText.mockClear();
  await copyAdjustments(sliders, keys);
  return writeText.mock.calls[0][0] as string;
}

/**
 * Every slider set to a value of its own, so a key written with its
 * neighbour's value cannot pass — which a document of mostly defaults, where
 * most values are 0, would let through.
 */
function distinctSliders(): Sliders {
  const sliders = { ...defaultSlidersRGB };
  let n = 0;
  for (const key of Object.keys(sliders) as (keyof Sliders)[]) {
    if (typeof sliders[key] === "boolean") (sliders as any)[key] = true;
    else (sliders as any)[key] = ++n + 0.125;
  }
  return sliders;
}

describe("every attribute", () => {
  it("belongs to exactly one mode", () => {
    expect([...normalKeys, ...darkroomKeys].sort()).toEqual(
      Object.keys(defaultSlidersRGB).sort(),
    );
    expect(normalKeys.filter((key) => darkroomKeys.includes(key))).toEqual([]);
  });

  it("is written, each with its own value and nothing else alongside", async () => {
    const sliders = distinctSliders();

    for (const keys of [normalKeys, darkroomKeys]) {
      const lines = (await copied(sliders, keys)).trimEnd().split("\n");
      expect(lines).toHaveLength(keys.length);
      for (const key of keys) {
        expect(lines).toContain(`${key}: ${String(sliders[key])}`);
      }
    }
  });

  it("is read back as the value it was given", async () => {
    const sliders = distinctSliders();

    for (const keys of [normalKeys, darkroomKeys]) {
      const parsed = parseAdjustments(await copied(sliders, keys), keys);
      expect(Object.keys(parsed)).toHaveLength(keys.length);
      for (const key of keys) expect(parsed[key]).toBe(sliders[key]);
    }
  });
});

describe("round trip", () => {
  it("returns every value it was given", async () => {
    const sliders: Sliders = {
      ...defaultSlidersRGB,
      invert: true,
      exposure: -1.25,
      contrast: 40,
      redGamma: 0.85,
      rgbOutputWhite: 240,
    };

    for (const keys of [normalKeys, darkroomKeys]) {
      const parsed = parseAdjustments(await copied(sliders, keys), keys);
      expect(Object.keys(parsed).sort()).toEqual([...keys].sort());
      for (const key of keys) expect(parsed[key]).toBe(sliders[key]);
    }
  });

  it("writes only the keys of the mode it was asked for", async () => {
    const text = await copied(defaultSlidersRGB, darkroomKeys);
    expect(text).toContain("redGamma:");
    expect(text).not.toContain("exposure:");
  });

  it("trims the float noise an animated value carries", async () => {
    const text = await copied(
      { ...defaultSlidersRGB, exposure: 0.30000000000000004 },
      normalKeys,
    );
    expect(text).toContain("exposure: 0.3");
  });
});

describe("reading an edited document", () => {
  const document = [
    "# saved from Open Darkroom",
    "---",
    "exposure: -1.25   # a stop and a quarter down",
    'contrast: "40"',
    "   brightness:   12   ",
    "",
    "unknownKey: 7",
  ].join("\r\n");

  it("takes comments, markers, quotes and stray whitespace", () => {
    expect(parseAdjustments(document, normalKeys)).toEqual({
      exposure: -1.25,
      contrast: 40,
      brightness: 12,
    });
  });

  it("reads booleans case-insensitively", () => {
    expect(parseAdjustments("invert: TRUE", darkroomKeys)).toEqual({
      invert: true,
    });
  });

  it("drops values of the wrong shape rather than the whole document", () => {
    expect(parseAdjustments("exposure: sunny\ncontrast: 40", normalKeys)).toEqual({
      contrast: 40,
    });
  });

  it("rejects text that is not a mapping", () => {
    expect(() => parseAdjustments("just some words", normalKeys)).toThrow();
  });

  it("rejects a document written for the other mode", () => {
    expect(() => parseAdjustments("redGamma: 0.85", normalKeys)).toThrow();
  });
});
