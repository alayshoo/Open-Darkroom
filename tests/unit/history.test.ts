// tests/unit/history.test.ts

import { describe, expect, it } from "vitest";
import { history } from "$lib/history/history.svelte";
import { FULL_FRAME } from "$lib/types/crop";
import type { Action } from "$lib/types/historyActions";

const slider = (v: number): Action => ({ type: "slider", key: "clarity", oldValue: v - 1, newValue: v });
const crop: Action = { type: "crop", oldValue: FULL_FRAME, newValue: { ...FULL_FRAME, w: 0.5 } };

describe("history.discard", () => {
    it("drops matching actions and keeps the pointer on the same step", () => {
        history.clear();
        [slider(1), crop, slider(2), crop, slider(3)].forEach((a) => history.push(a));
        history.undo();

        history.discard((a) => a.type === "crop");

        expect(history.stack).toEqual([slider(1), slider(2), slider(3)]);
        expect(history.undo()).toEqual(slider(2));
        expect(history.redo()).toEqual(slider(2));
        expect(history.redo()).toEqual(slider(3));
        expect(history.redo()).toBeNull();
    });
});
