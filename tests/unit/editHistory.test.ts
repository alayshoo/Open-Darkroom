// tests/unit/editHistory.test.ts

import { describe, expect, it } from "vitest";
import { FULL_FRAME } from "$lib/types/crop";
import { defaultCurves } from "$lib/types/curves";
import type { Action } from "$lib/types/historyActions";
import type { Retouch } from "$lib/types/retouch";
import { parseEditHistory, serializeEditHistory, trimEditHistory } from "$lib/types/editHistory";

const slider = (v: number): Action => ({ type: "slider", key: "exposure", oldValue: v - 1, newValue: v });
const stroke: Retouch = { mode: "heal", points: [[0.5, 0.5]], radius: 0.01, feather: 0.5, opacity: 1, offset: [0.1, 0] };

const every: Action[] = [
    slider(1),
    { type: "sliders", oldValues: { contrast: 0 }, newValues: { contrast: 20 }, oldCurves: defaultCurves(), newCurves: defaultCurves() },
    { type: "curves", oldCurves: defaultCurves(), newCurves: defaultCurves() },
    { type: "crop", oldValue: FULL_FRAME, newValue: { ...FULL_FRAME, w: 0.5 } },
    { type: "retouchAdd", stroke },
    { type: "retouchClear", strokes: [stroke] },
];

describe("the history stored with an edit", () => {
    it("reads back every kind of step, and where it stood", () => {
        const text = serializeEditHistory({ steps: every, pointer: 3 });
        expect(parseEditHistory(text)).toEqual({ steps: every, pointer: 3 });
    });

    it("is dropped whole when any step cannot be read", () => {
        const text = JSON.stringify({ version: 1, pointer: 1, steps: [slider(1), { type: "smudge" }] });
        expect(parseEditHistory(text)).toBeNull();
        expect(parseEditHistory(JSON.stringify({ version: 1, pointer: 0, steps: [{ ...slider(1), key: "sparkle" }] }))).toBeNull();
        expect(parseEditHistory(JSON.stringify({ version: 1, pointer: 4, steps: [slider(1)] }))).toBeNull();
        expect(parseEditHistory("not json")).toBeNull();
        expect(parseEditHistory(null)).toBeNull();
    });

    it("keeps an empty or fully undone history", () => {
        expect(parseEditHistory(serializeEditHistory({ steps: [], pointer: -1 }))).toEqual({ steps: [], pointer: -1 });
        expect(parseEditHistory(serializeEditHistory({ steps: [slider(1)], pointer: -1 }))?.pointer).toBe(-1);
    });

    it("keeps the latest steps when there are too many, and the step the edit is on", () => {
        const steps = [1, 2, 3, 4, 5].map(slider);
        expect(trimEditHistory({ steps, pointer: 4 }, 3)).toEqual({ steps: steps.slice(2), pointer: 2 });
        expect(trimEditHistory({ steps, pointer: 1 }, 3)).toEqual({ steps: steps.slice(0, 3), pointer: 1 });
        expect(trimEditHistory({ steps, pointer: 2 }, 2)).toEqual({ steps: steps.slice(1, 3), pointer: 1 });
    });
});
