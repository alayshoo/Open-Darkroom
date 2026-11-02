// tests/unit/historyControls.test.ts
//
// Undo and redo bring the edited controls into view: the mode they belong to
// and, in lighter-room, the page they sit on. The page itself is checked as
// text, the way sharpnessPage.test.ts does, so the page map cannot drift from
// the markup and every darkroom control is known to reach the history.

import { describe, expect, it } from "vitest";
import {
    actionMode,
    actionPage,
    CURVES_PAGE,
    LIGHTER_ROOM_PAGES,
} from "$lib/history/actionControls";
import { HSL_KEYS, isDarkroomKey, modeKeys, type Sliders } from "$lib/types/imgParameters";
import { defaultCurves } from "$lib/types/curves";
import { FULL_FRAME } from "$lib/types/crop";
import type { Action } from "$lib/types/historyActions";

import PAGE from "../../src/routes/+page.svelte?raw";

const slider = (key: keyof Sliders): Action => ({ type: "slider", key, oldValue: 0, newValue: 1 });
const sliders = (keys: (keyof Sliders)[], withCurves = false): Action => ({
    type: "sliders",
    oldValues: Object.fromEntries(keys.map((k) => [k, 0])),
    newValues: Object.fromEntries(keys.map((k) => [k, 1])),
    ...(withCurves ? { oldCurves: defaultCurves(), newCurves: defaultCurves() } : {}),
});
const curves: Action = { type: "curves", oldCurves: defaultCurves(), newCurves: defaultCurves() };
const crop: Action = { type: "crop", oldValue: FULL_FRAME, newValue: { ...FULL_FRAME, w: 0.5 } };

describe("actionMode", () => {
    it("puts every darkroom slider in the darkroom", () => {
        for (const key of modeKeys(true)) expect(actionMode(slider(key)), key).toBe("darkroom");
    });

    it("puts every other slider in lighter-room", () => {
        for (const key of modeKeys(false)) expect(actionMode(slider(key)), key).toBe("lighter-room");
    });

    it("follows the keys of a paste or reset", () => {
        expect(actionMode(sliders(modeKeys(true)))).toBe("darkroom");
        expect(actionMode(sliders(modeKeys(false)))).toBe("lighter-room");
    });

    it("sends any step that carries curves to lighter-room", () => {
        expect(actionMode(curves)).toBe("lighter-room");
        expect(actionMode(sliders([], true))).toBe("lighter-room");
        expect(actionMode(sliders(["redGamma"], true))).toBe("lighter-room");
    });

    it("leaves steps shared by both modes alone", () => {
        expect(actionMode(crop)).toBeNull();
        expect(actionMode(sliders([]))).toBeNull();
    });
});

describe("actionPage", () => {
    it("finds each lighter-room slider from a page that lacks it", () => {
        for (const key of modeKeys(false)) {
            const home = LIGHTER_ROOM_PAGES.findIndex((page) => page.includes(key));
            const away = LIGHTER_ROOM_PAGES.findIndex((page) => !page.includes(key));
            expect(actionPage(slider(key), away), key).toBe(home);
        }
    });

    it("stays on a page that already shows the slider", () => {
        LIGHTER_ROOM_PAGES.forEach((page, i) => {
            for (const key of page) expect(actionPage(slider(key), i), key).toBeNull();
        });
    });

    it("stays on either page for a slider shown on both", () => {
        expect(actionPage(slider("vibrance"), 1)).toBeNull();
        expect(actionPage(slider("vibrance"), 2)).toBeNull();
        expect(actionPage(slider("saturation"), 0)).toBe(1);
    });

    it("goes to the curves page for a curves edit", () => {
        expect(actionPage(curves, 0)).toBe(CURVES_PAGE);
        expect(actionPage(curves, CURVES_PAGE)).toBeNull();
        expect(actionPage(sliders(["clarity"], true), 1)).toBe(0);
    });

    it("stays put for a set with a key on the current page", () => {
        expect(actionPage(sliders(modeKeys(false)), 0)).toBeNull();
        expect(actionPage(sliders(modeKeys(false)), 1)).toBeNull();
        expect(actionPage(sliders(HSL_KEYS), 1)).toBe(2);
    });

    it("has no page for controls outside lighter-room", () => {
        expect(actionPage(slider("redGamma"), 1)).toBeNull();
        expect(actionPage(crop, 1)).toBeNull();
    });
});

describe("lighter-room page map", () => {
    const mapped = new Set(LIGHTER_ROOM_PAGES.flat());

    it("places every lighter-room slider and no darkroom one", () => {
        expect(mapped).toEqual(new Set(modeKeys(false)));
        for (const key of mapped) expect(isDarkroomKey(key), key).toBe(false);
    });

    /** The markup between two markers. */
    function section(from: string, to: string): string {
        const start = PAGE.indexOf(from);
        const end = PAGE.indexOf(to, start);
        if (start < 0 || end < 0) throw new Error(`markers ${from} … ${to} not found in +page.svelte`);
        return PAGE.slice(start, end);
    }

    const PANELS = [
        section("<!-- Sharpness Panel -->", "<!-- Light & Color Panel -->"),
        section("<!-- Light & Color Panel -->", "<!-- Curves, HSL & Colour Mixer Panel -->"),
        section("<!-- Curves, HSL & Colour Mixer Panel -->", "bind:this={pagerElDarkroom}"),
    ];

    it("matches the sliders each panel binds", () => {
        expect(PANELS.length).toBe(LIGHTER_ROOM_PAGES.length);
        PANELS.forEach((markup, i) => {
            const bound = new Set([...markup.matchAll(/bind:value=\{sliders\.(\w+)\}/g)].map((m) => m[1]));
            if (markup.includes("<HslBandGroup")) HSL_KEYS.forEach((k) => bound.add(k));
            expect(bound, `page ${i}`).toEqual(new Set(LIGHTER_ROOM_PAGES[i]));
        });
    });

    it("puts the curve editor on the curves page", () => {
        PANELS.forEach((markup, i) => {
            expect(markup.includes("<CurveEditor"), `page ${i}`).toBe(i === CURVES_PAGE);
        });
    });
});

describe("darkroom history wiring", () => {
    interface Binding { group: string; slot: string; bound: string; committed: string }

    /** Every bound value of the multi-value slider groups, with the key its commit handler records. */
    function bindings(markup: string): Binding[] {
        const out: Binding[] = [];
        for (const [, tag, body] of markup.matchAll(
            /<(TripleValSliderHistGroup|TripleValSliderGroup|DoubleValSliderGroup)\b([\s\S]*?)><\/\1>/g,
        )) {
            const group = `${tag} "${body.match(/name="([^"]*)"/)?.[1] ?? "?"}"`;
            for (const [, slot, bound] of body.matchAll(/bind:value([ABC])=\{sliders\.(\w+)\}/g)) {
                const handler = body.match(new RegExp(`onCommit${slot}=\\{[^}]*key:\\s*"(\\w+)"`));
                out.push({ group, slot, bound, committed: handler?.[1] ?? "" });
            }
        }
        return out;
    }

    const ALL = bindings(PAGE);
    const DARKROOM = bindings(PAGE.slice(PAGE.indexOf("bind:this={pagerElDarkroom}")));

    it("gives every value a commit handler under its own key", () => {
        expect(ALL.length).toBeGreaterThan(0);
        for (const b of ALL) {
            expect(b.committed, `${b.group} value ${b.slot} commits nothing`).not.toBe("");
            expect(b.committed, `${b.group} value ${b.slot} commits the wrong key`).toBe(b.bound);
        }
    });

    it("records every darkroom control", () => {
        const committed = new Set(DARKROOM.map((b) => b.committed));
        // The invert toggle sits above the pager and commits through handleInvertToggle.
        expect(PAGE).toMatch(/function handleInvertToggle[\s\S]*?key:\s*"invert"/);
        committed.add("invert");
        expect(committed).toEqual(new Set(modeKeys(true)));
    });
});
