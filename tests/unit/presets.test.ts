// tests/unit/presets.test.ts

import { describe, expect, it, vi } from "vitest";
import { defaultSlidersRGB, type Sliders } from "$lib/types/imgParameters";
import { defaultCurves } from "$lib/types/curves";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn(), readText: vi.fn() }));

const {
  PRESET_FIELDS,
  PRESET_GROUPS,
  captureFields,
  changedFieldIds,
  modeFieldIds,
  parsePreset,
  presetFieldIds,
  revisePreset,
  serializePreset,
} = await import("$lib/presets/presetDocument");

const ALL_FIELDS = PRESET_FIELDS.map((f) => f.id);

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

describe("preset fields", () => {
  it("cover every slider and curve channel exactly once", () => {
    const expected = [
      ...Object.keys(defaultSlidersRGB),
      ...["master", "red", "green", "blue"].map((c) => `curve:${c}`),
    ];
    expect([...ALL_FIELDS].sort()).toEqual(expected.sort());
    expect(new Set(ALL_FIELDS).size).toBe(ALL_FIELDS.length);
  });

  it("list every row's fields as the group's own", () => {
    for (const group of PRESET_GROUPS) {
      if (group.rows) expect(group.rows.flatMap((r) => r.fields)).toEqual(group.fields);
    }
  });

  it("give each mode its own fields plus the shared ones", () => {
    expect(modeFieldIds("darkroom").has("invert")).toBe(true);
    expect(modeFieldIds("darkroom").has("usmAmount")).toBe(true);
    expect(modeFieldIds("darkroom").has("exposure")).toBe(false);
    expect(modeFieldIds("lighter-room").has("curve:red")).toBe(true);
  });
});

describe("capturing", () => {
  it("takes only the chosen sliders", () => {
    const captured = captureFields(["exposure", "invert"], distinctSliders(), defaultCurves());
    expect(Object.keys(captured.sliders).sort()).toEqual(["exposure", "invert"]);
    expect(captured.curves).toBeUndefined();
  });

  it("takes single curve channels", () => {
    const captured = captureFields(["curve:red"], defaultSlidersRGB, bentCurves());
    expect(captured).toEqual({ sliders: {}, curves: { red: bentCurves().red } });
  });

  it("reports the fields a preset holds", () => {
    const captured = captureFields(["shadows", "curve:red", "rgbOutputWhite"], distinctSliders(), bentCurves());
    expect([...presetFieldIds({ name: "x", ...captured })].sort()).toEqual(
      ["curve:red", "rgbOutputWhite", "shadows"].sort(),
    );
  });

  it("finds the fields moved off their defaults", () => {
    const sliders = { ...defaultSlidersRGB, exposure: 0.5, usmAmount: 40 };
    expect([...changedFieldIds(sliders, defaultCurves())].sort()).toEqual(["exposure", "usmAmount"]);
    expect([...changedFieldIds(defaultSlidersRGB, bentCurves())]).toEqual(["curve:red"]);
    expect(changedFieldIds(defaultSlidersRGB, defaultCurves()).size).toBe(0);
  });
});

describe("revising", () => {
  const stored = { name: "Old", sliders: { exposure: 1.5, usmAmount: 80 } };
  const current = captureFields(
    ALL_FIELDS,
    { ...defaultSlidersRGB, exposure: -2, contrast: 20, wbTemp: 6000 },
    bentCurves(),
  );

  it("keeps stored values and drops unticked ones", () => {
    expect(revisePreset(stored, current, new Set(["exposure"]))).toEqual({ sliders: { exposure: 1.5 } });
  });

  it("takes the current values for fields new to the preset", () => {
    const revised = revisePreset(stored, current, new Set(["exposure", "wbTemp", "curve:red"]));
    expect(revised).toEqual({ sliders: { exposure: 1.5, wbTemp: 6000 }, curves: { red: bentCurves().red } });
  });
});

describe("the document", () => {
  it("round-trips a full preset", () => {
    const preset = {
      name: "Portra 400 #2",
      ...captureFields(ALL_FIELDS, distinctSliders(), bentCurves()),
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
