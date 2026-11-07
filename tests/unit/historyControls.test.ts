// tests/unit/historyControls.test.ts
//
// Undo and redo bring the edited controls into view: the mode they belong to
// and the page they sit on. The controls are also checked as text, the way
// sharpnessPage.test.ts does, so the page maps cannot drift from the markup.

import { describe, expect, it } from "vitest";
import {
    actionMode,
    actionPage,
    CURVES_PAGE,
    DARKROOM_PAGES,
    LIGHTER_ROOM_PAGES,
} from "$lib/history/actionControls";
import {
    HSL_KEYS,
    isDarkroomKey,
    modeKeys,
    sharedModeKeys,
    type Sliders,
} from "$lib/types/imgParameters";
import { defaultCurves } from "$lib/types/curves";
import { FULL_FRAME } from "$lib/types/crop";
import type { Action } from "$lib/types/historyActions";

import LIGHTER from "../../src/lib/components/editor/LighterRoomPages.svelte?raw";
import DARKROOM from "../../src/lib/components/editor/DarkroomPages.svelte?raw";
import UNSHARP_MASK from "../../src/lib/components/editor/UnsharpMask.svelte?raw";
import EDITOR from "../../src/lib/editor/editor.svelte.ts?raw";

const slider = (key: keyof Sliders): Action => ({ type: "slider", key, oldValue: 0, newValue: 1 });
const sliders = (keys: (keyof Sliders)[], withCurves = false): Action => ({
    type: "sliders",
    oldValues: Object.fromEntries(keys.map((k) => [k, 0])),
    newValues: Object.fromEntries(keys.map((k) => [k, 1])),
    ...(withCurves ? { oldCurves: defaultCurves(), newCurves: defaultCurves() } : {}),
});
const curves: Action = { type: "curves", oldCurves: defaultCurves(), newCurves: defaultCurves() };
const crop: Action = { type: "crop", oldValue: FULL_FRAME, newValue: { ...FULL_FRAME, w: 0.5 } };

const darkroomOnly = modeKeys(true).filter((k) => !sharedModeKeys.has(k));
const lighterRoomOnly = modeKeys(false).filter((k) => !sharedModeKeys.has(k));
const shared = [...sharedModeKeys];

describe("actionMode", () => {
    it("puts every darkroom-only slider in the darkroom", () => {
        for (const key of darkroomOnly) expect(actionMode(slider(key)), key).toBe("darkroom");
    });

    it("puts every lighter-room-only slider in lighter-room", () => {
        for (const key of lighterRoomOnly) expect(actionMode(slider(key)), key).toBe("lighter-room");
    });

    it("leaves the sliders both modes show to the current mode", () => {
        for (const key of shared) expect(actionMode(slider(key)), key).toBeNull();
        expect(actionMode(sliders(shared))).toBeNull();
    });

    it("follows the keys of a paste or reset, shared ones included", () => {
        expect(actionMode(sliders(modeKeys(true)))).toBe("darkroom");
        expect(actionMode(sliders(modeKeys(false)))).toBe("lighter-room");
    });

    it("sends any step that carries curves to lighter-room", () => {
        expect(actionMode(curves)).toBe("lighter-room");
        expect(actionMode(sliders([], true))).toBe("lighter-room");
        expect(actionMode(sliders(["redGamma"], true))).toBe("lighter-room");
    });

    it("leaves steps with no controls of either mode alone", () => {
        expect(actionMode(crop)).toBeNull();
        expect(actionMode(sliders([]))).toBeNull();
    });
});

const MODES = [
    ["lighter-room", LIGHTER_ROOM_PAGES],
    ["darkroom", DARKROOM_PAGES],
] as const;

describe("actionPage", () => {
    for (const [mode, pages] of MODES) {
        it(`finds each ${mode} slider from a page that lacks it`, () => {
            for (const key of new Set(pages.flat())) {
                const home = pages.findIndex((page) => page.includes(key));
                const away = pages.findIndex((page) => !page.includes(key));
                expect(actionPage(slider(key), mode, away), key).toBe(home);
            }
        });

        it(`stays on a ${mode} page that already shows the slider`, () => {
            pages.forEach((page, i) => {
                for (const key of page) expect(actionPage(slider(key), mode, i), key).toBeNull();
            });
        });
    }

    it("stays on either page for a slider shown on both", () => {
        expect(actionPage(slider("vibrance"), "lighter-room", 1)).toBeNull();
        expect(actionPage(slider("vibrance"), "lighter-room", 2)).toBeNull();
        expect(actionPage(slider("saturation"), "lighter-room", 0)).toBe(1);
    });

    it("finds a shared slider's page in whichever mode shows it", () => {
        expect(actionPage(slider("usmRadius"), "lighter-room", 1)).toBe(0);
        expect(actionPage(slider("usmRadius"), "darkroom", 1)).toBe(0);
    });

    it("goes to the curves page for a curves edit", () => {
        expect(actionPage(curves, "lighter-room", 0)).toBe(CURVES_PAGE);
        expect(actionPage(curves, "lighter-room", CURVES_PAGE)).toBeNull();
        expect(actionPage(sliders(["clarity"], true), "lighter-room", 1)).toBe(0);
    });

    it("stays put for a set with a key on the current page", () => {
        expect(actionPage(sliders(modeKeys(false)), "lighter-room", 0)).toBeNull();
        expect(actionPage(sliders(modeKeys(false)), "lighter-room", 1)).toBeNull();
        expect(actionPage(sliders(modeKeys(true)), "darkroom", 0)).toBeNull();
        expect(actionPage(sliders(HSL_KEYS), "lighter-room", 1)).toBe(2);
    });

    it("has no page for controls the mode does not show", () => {
        expect(actionPage(slider("redGamma"), "lighter-room", 1)).toBeNull();
        expect(actionPage(slider("exposure"), "darkroom", 1)).toBeNull();
        expect(actionPage(slider("invert"), "darkroom", 0)).toBeNull();
        expect(actionPage(crop, "lighter-room", 1)).toBeNull();
    });
});

/** The markup between two markers, or to the end, with the shared unsharp mask in place. */
function section(markup: string, from: string, to?: string): string {
    const start = markup.indexOf(from);
    const end = to ? markup.indexOf(to, start) : markup.length;
    if (start < 0 || end < 0) throw new Error(`markers ${from} … ${to} not found`);
    return markup.slice(start, end).replaceAll("<UnsharpMask />", UNSHARP_MASK);
}

const PANELS = {
    "lighter-room": [
        section(LIGHTER, "<!-- Sharpness Panel -->", "<!-- Light & Color Panel -->"),
        section(LIGHTER, "<!-- Light & Color Panel -->", "<!-- Curves, HSL & Colour Mixer Panel -->"),
        section(LIGHTER, "<!-- Curves, HSL & Colour Mixer Panel -->", "<style>"),
    ],
    darkroom: [
        section(DARKROOM, "<!-- Darkroom Sharpness Panel -->", "<!-- Input Levels Panel -->"),
        section(DARKROOM, "<!-- Input Levels Panel -->"),
    ],
};

describe("page maps", () => {
    it("place every slider of a mode, and Invert on no page", () => {
        expect(new Set(LIGHTER_ROOM_PAGES.flat())).toEqual(new Set(modeKeys(false)));
        expect(new Set([...DARKROOM_PAGES.flat(), "invert"])).toEqual(new Set(modeKeys(true)));
        for (const key of LIGHTER_ROOM_PAGES.flat()) expect(isDarkroomKey(key), key).toBe(false);
    });

    for (const [mode, pages] of MODES) {
        it(`match the sliders each ${mode} panel binds`, () => {
            const panels = PANELS[mode];
            expect(panels.length).toBe(pages.length);
            panels.forEach((markup, i) => {
                const bound = new Set(
                    [...markup.matchAll(/bind:value[ABC]?=\{editor\.edit\.sliders\.(\w+)\}/g)].map((m) => m[1]),
                );
                if (markup.includes("<HslBandGroup")) HSL_KEYS.forEach((k) => bound.add(k));
                expect(bound, `${mode} page ${i}`).toEqual(new Set(pages[i]));
            });
        });
    }

    it("put the curve editor on the curves page only", () => {
        [...PANELS["lighter-room"], ...PANELS.darkroom].forEach((markup, i) => {
            expect(markup.includes("<CurveEditor"), `panel ${i}`).toBe(i === CURVES_PAGE);
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
            for (const [, slot, bound] of body.matchAll(/bind:value([ABC])=\{editor\.edit\.sliders\.(\w+)\}/g)) {
                const handler = body.match(new RegExp(`onCommit${slot}=\\{[^}]*key:\\s*"(\\w+)"`));
                out.push({ group, slot, bound, committed: handler?.[1] ?? "" });
            }
        }
        return out;
    }

    it("gives every value a commit handler under its own key", () => {
        const all = bindings(LIGHTER + DARKROOM);
        expect(all.length).toBeGreaterThan(0);
        for (const b of all) {
            expect(b.committed, `${b.group} value ${b.slot} commits nothing`).not.toBe("");
            expect(b.committed, `${b.group} value ${b.slot} commits the wrong key`).toBe(b.bound);
        }
    });

    it("records every darkroom control", () => {
        const markup = PANELS.darkroom.join("");
        const committed = new Set([
            ...bindings(markup).map((b) => b.committed),
            ...[...markup.matchAll(/<SingleValSliderGroup\b[\s\S]*?key:\s*"(\w+)"/g)].map((m) => m[1]),
        ]);
        // The invert toggle sits above the pager and commits through toggleInvert.
        expect(EDITOR).toMatch(/toggleInvert = [\s\S]*?key:\s*"invert"/);
        committed.add("invert");
        expect(committed).toEqual(new Set(modeKeys(true)));
    });
});
