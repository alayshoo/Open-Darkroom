// tests/unit/sharpnessPage.test.ts
//
// The sharpness panel is markup, so this reads the page as text and checks the
// wiring the compiler cannot: that each slider binds its own key, that the
// history entry it commits names that same key, and that its rails and reset
// value agree with the defaults the app starts from.
//
// The bug this exists to catch is the copy-paste one — a duplicated slider block
// left pointing at the field above it. It type-checks, it renders, and it moves
// the wrong control.

import { describe, expect, it } from "vitest";
import {
  defaultSlidersBW,
  defaultSlidersRGB,
  overridesBW,
  type Sliders,
} from "$lib/types/imgParameters";

import PAGE from "../../src/routes/+page.svelte?raw";

interface SliderBlock {
  name: string;
  unit: string;
  boundKey: string;
  historyKey: string;
  numbers: Record<string, number>;
}

/** The markup between two comment markers. */
function section(from: string, to: string): string {
  const start = PAGE.indexOf(from);
  if (start < 0) throw new Error(`marker ${from} not found in +page.svelte`);
  const end = PAGE.indexOf(to, start);
  if (end < 0) throw new Error(`marker ${to} not found after ${from}`);
  return PAGE.slice(start, end);
}

/** Every `<SingleValSliderGroup>` in `markup`, in source order. */
function sliderBlocks(markup: string): SliderBlock[] {
  return markup
    .split("<SingleValSliderGroup")
    .slice(1)
    .map((raw) => {
      const block = raw.slice(0, raw.indexOf("</SingleValSliderGroup>"));
      const one = (re: RegExp) => block.match(re)?.[1] ?? "";

      const numbers: Record<string, number> = {};
      for (const [, prop, value] of block.matchAll(
        /\b(min|max|defaultValue|decimalPlaces|dragStep|sliderStep|keyboardStep)=\{(-?[\d.]+)\}/g,
      )) {
        numbers[prop] = Number(value);
      }

      return {
        name: one(/name="([^"]*)"/),
        unit: one(/unit="([^"]*)"/),
        boundKey: one(/bind:value=\{sliders\.(\w+)\}/),
        historyKey: one(/key:\s*"(\w+)"/),
        numbers,
      };
    });
}

const SHARPNESS = sliderBlocks(
  section("<!-- Sharpness Panel -->", "<!-- Light & Color Panel -->"),
);

/** Name → slider key, in the order the panel lays them out. */
const EXPECTED: [string, keyof Sliders][] = [
  ["Clarity", "clarity"],
  ["Texture", "texture"],
  ["Amount", "usmAmount"],
  ["Radius", "usmRadius"],
  ["Luma Threshold", "usmLumaThreshold"],
  ["Detail Threshold", "usmDetailThreshold"],
];

describe("sharpness panel wiring", () => {
  it("lays out the six sharpness sliders in order", () => {
    expect(SHARPNESS.map((s) => s.name)).toEqual(EXPECTED.map(([name]) => name));
  });

  it("binds each slider to its own field", () => {
    expect(SHARPNESS.map((s) => s.boundKey)).toEqual(EXPECTED.map(([, key]) => key));
  });

  it("commits history under the field it edits", () => {
    for (const slider of SHARPNESS) {
      expect(slider.historyKey, `${slider.name} commits the wrong key`).toBe(
        slider.boundKey,
      );
    }
  });

  it("gives every slider a unit and a label", () => {
    for (const slider of SHARPNESS) {
      expect(slider.name).not.toBe("");
      expect(slider.unit).not.toBe("");
    }
  });
});

// The same mis-wiring is possible on any panel, and the parser is already here,
// so the history check is worth running over the whole page. The Hue slider
// committed under "vibrance" until this test was written.
describe("every slider on the page", () => {
  const ALL = sliderBlocks(PAGE);

  it("finds a commit handler on each one", () => {
    expect(ALL.length).toBeGreaterThan(SHARPNESS.length);
    for (const slider of ALL) {
      expect(slider.boundKey, `${slider.name} binds nothing`).not.toBe("");
      expect(slider.historyKey, `${slider.name} commits nothing`).not.toBe("");
    }
  });

  it("commits history under the field it edits", () => {
    // A slider that commits its neighbour's key undoes the wrong control.
    for (const slider of ALL) {
      expect(slider.historyKey, `${slider.name} commits the wrong key`).toBe(
        slider.boundKey,
      );
    }
  });
});

describe("sharpness slider ranges", () => {
  it("declares a non-empty range containing the reset value", () => {
    for (const { name, numbers } of SHARPNESS) {
      const { min, max, defaultValue } = numbers;
      expect([min, max, defaultValue].every(Number.isFinite), name).toBe(true);
      expect(min, name).toBeLessThan(max);
      expect(defaultValue, name).toBeGreaterThanOrEqual(min);
      expect(defaultValue, name).toBeLessThanOrEqual(max);
    }
  });

  it("resets to the value the app starts from", () => {
    // Backspace over a slider sets it to `defaultValue`; if that disagrees with
    // defaultSlidersRGB, resetting the panel lands somewhere the app never opens
    // at, and the render differs from a freshly loaded image.
    for (const { name, boundKey, numbers } of SHARPNESS) {
      expect(numbers.defaultValue, name).toBe(
        defaultSlidersRGB[boundKey as keyof Sliders],
      );
    }
  });

  it("steps at least as coarsely as the readout can show", () => {
    // A step finer than the displayed precision moves the value without moving
    // the number on screen.
    for (const { name, numbers } of SHARPNESS) {
      const resolution = Math.pow(10, -numbers.decimalPlaces);
      for (const step of ["dragStep", "sliderStep", "keyboardStep"] as const) {
        expect(numbers[step], `${name}: ${step}`).toBeGreaterThanOrEqual(resolution);
      }
    }
  });
});

describe("sharpness defaults", () => {
  const keys = EXPECTED.map(([, key]) => key);

  it("starts neutral — an image opens unsharpened", () => {
    expect(defaultSlidersRGB.clarity).toBe(0);
    expect(defaultSlidersRGB.texture).toBe(0);
    expect(defaultSlidersRGB.usmAmount).toBe(0);
    expect(defaultSlidersRGB.usmLumaThreshold).toBe(0);
    expect(defaultSlidersRGB.usmDetailThreshold).toBe(0);
    // Amount 0 switches the mask off, so radius has no neutral value of its own
    // and starts at a usable kernel width instead.
    expect(defaultSlidersRGB.usmRadius).toBeGreaterThan(0);
  });

  it("is the same in black and white — sharpness is not a colour control", () => {
    for (const key of keys) {
      expect(defaultSlidersBW[key], key).toBe(defaultSlidersRGB[key]);
      expect(overridesBW, key).not.toHaveProperty(key);
    }
  });
});
