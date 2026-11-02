// tests/unit/presets.test.ts

import { describe, expect, it, vi } from "vitest";
import { defaultSlidersRGB, type Sliders } from "$lib/types/imgParameters";
import { defaultCurves } from "$lib/types/curves";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn(), readText: vi.fn() }));

const {
  PRESET_GROUPS,
  captureGroups,
  changedGroupIds,
  parsePreset,
  presetGroupIds,
  revisePreset,
  serializePreset,
} = await import("$lib/presets/presetDocument");

function distinctSliders(): Sliders {
  const sliders = { ...defaultSlidersRGB };
  let n = 0;
  for (const key of Object.keys(sliders) as (keyof Sliders)[]) {
    if (typeof sliders[key] === "boolean") (sliders as any)[key] = true;
    else (sliders as any)[key] = ++n + 0.125;
  }
  return sliders;
}

function bentCurves() {
  const curves = defaultCurves();
  curves.red = [
    { x: 0, y: 0.1 },
    { x: 0.5, y: 0.6 },
    { x: 1, y: 0.9 },
  ];
  return curves;
}

describe("preset groups", () => {
  it("hold every slider exactly once", () => {
    const keys = PRESET_GROUPS.flatMap((g) => g.keys);
    expect([...keys].sort()).toEqual(Object.keys(defaultSlidersRGB).sort());
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("have one group for the curves", () => {
    expect(PRESET_GROUPS.filter((g) => g.curves)).toHaveLength(1);
  });

  it("have unique ids", () => {
    const ids = PRESET_GROUPS.map((g) => g.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("capturing", () => {
  it("takes only the chosen groups", () => {
    const captured = captureGroups(["light", "invert"], distinctSliders(), defaultCurves());
    expect(Object.keys(captured.sliders).sort()).toEqual(
      ["brightness", "contrast", "exposure", "invert"].sort(),
    );
    expect(captured.curves).toBeUndefined();
  });

  it("takes all four curves with the curves group", () => {
    const captured = captureGroups(["curves"], defaultSlidersRGB, bentCurves());
    expect(captured.sliders).toEqual({});
    expect(captured.curves).toEqual(bentCurves());
  });

  it("reports the groups a preset holds", () => {
    const captured = captureGroups(["tone", "curves", "rgbOutput"], distinctSliders(), bentCurves());
    expect([...presetGroupIds({ name: "x", ...captured })].sort()).toEqual(
      ["curves", "rgbOutput", "tone"].sort(),
    );
  });

  it("finds the groups moved off their defaults", () => {
    const sliders = { ...defaultSlidersRGB, exposure: 0.5, usmAmount: 40 };
    expect([...changedGroupIds(sliders, defaultCurves())].sort()).toEqual(["light", "unsharpMask"]);
    expect([...changedGroupIds(defaultSlidersRGB, bentCurves())]).toEqual(["curves"]);
    expect(changedGroupIds(defaultSlidersRGB, defaultCurves()).size).toBe(0);
  });
});

describe("revising", () => {
  const stored = { name: "Old", sliders: { exposure: 1.5, usmAmount: 80 } };
  const current = captureGroups(
    PRESET_GROUPS.map((g) => g.id),
    { ...defaultSlidersRGB, exposure: -2, contrast: 20, wbTemp: 6000 },
    bentCurves(),
  );

  it("keeps stored values, leaving out what the stored group lacked", () => {
    expect(revisePreset(stored, current, new Set(["light"]))).toEqual({ sliders: { exposure: 1.5 } });
  });

  it("takes the current values for groups new to the preset", () => {
    const revised = revisePreset(stored, current, new Set(["light", "whiteBalance", "curves"]));
    expect(revised.sliders).toEqual({ exposure: 1.5, wbTemp: 6000, wbTint: 0 });
    expect(revised.curves).toEqual(bentCurves());
  });
});

describe("the document", () => {
  it("round-trips a full preset", () => {
    const preset = {
      name: "Portra 400 #2",
      ...captureGroups(PRESET_GROUPS.map((g) => g.id), distinctSliders(), bentCurves()),
    };
    expect(parsePreset(serializePreset(preset))).toEqual(preset);
  });

  it("round-trips a name holding quotes", () => {
    const preset = { name: `The "warm" one`, sliders: { exposure: 1 } };
    expect(parsePreset(serializePreset(preset)).name).toBe(`The "warm" one`);
  });

  it("writes a name on one line", () => {
    const text = serializePreset({ name: "two\nlines", sliders: { exposure: 1 } });
    expect(parsePreset(text).name).toBe("two lines");
  });

  it("reads a clipboard copy, naming it from the fallback", () => {
    const parsed = parsePreset("exposure: 0.5\ncontrast: 10\n", "Imported");
    expect(parsed).toEqual({ name: "Imported", sliders: { exposure: 0.5, contrast: 10 } });
  });

  it("refuses a document with no adjustments", () => {
    expect(() => parsePreset('name: "Empty"\n')).toThrow();
  });
});
