// tests/unit/shortcuts.test.ts
//
// Two things have to hold for a rebindable shortcut: a binding survives the
// round trip through the settings file unchanged, and a key press picks out
// exactly one action. Both are pure functions over a store, so neither needs a
// DOM — the events below are the four fields the matcher actually reads.

import { beforeEach, describe, expect, it, vi } from "vitest";

import {
    display,
    matchesEvent,
    parse,
    serialize,
    shortcutActions,
    shortcutGroups,
    type Binding,
} from "$lib/config/shortcutRegistry";

const load = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-store", () => ({ load }));

/** Stands in for one file on disk, with the calls persistence.ts makes on it. */
function fileWith(contents: Record<string, unknown> = {}) {
    const data = new Map(Object.entries(contents));
    return {
        data,
        entries: vi.fn(async () => [...data.entries()]),
        set: vi.fn(async (key: string, value: unknown) => void data.set(key, value)),
        save: vi.fn(async () => {}),
    };
}

type Mods = Partial<Record<"ctrl" | "shift" | "alt" | "meta", true>>;

/** A key press, as far as the matcher is concerned. */
function press(key: string, mods: Mods = {}) {
    return {
        key,
        ctrlKey: mods.ctrl ?? false,
        shiftKey: mods.shift ?? false,
        altKey: mods.alt ?? false,
        metaKey: mods.meta ?? false,
    } as KeyboardEvent;
}

function binding(key: string, mods: Partial<Binding> = {}): Binding {
    return { key, ctrl: false, shift: false, alt: false, meta: false, ...mods };
}

describe("a binding written down", () => {
    it("comes back the same", () => {
        for (const b of [
            binding("z", { ctrl: true }),
            binding("z", { ctrl: true, shift: true }),
            binding("r", { ctrl: true, alt: true }),
            binding("F1"),
            binding(","),
            binding("ArrowLeft", { meta: true }),
        ]) {
            expect(parse(serialize(b))).toEqual(b);
        }
    });

    it("survives being the plus key itself", () => {
        const b = binding("+", { ctrl: true });
        expect(serialize(b)).toBe("ctrl++");
        expect(parse("ctrl++")).toEqual(b);
        expect(parse("+")).toEqual(binding("+"));
    });

    it("is nothing at all when there is no key left in it", () => {
        expect(parse("")).toBeNull();
    });

    it("reads as the modifiers in a fixed order, then the key", () => {
        expect(display("ctrl+shift+z")).toBe("Ctrl+Shift+Z");
        expect(display("ctrl+alt+r")).toBe("Ctrl+Alt+R");
        expect(display("F1")).toBe("F1");
        expect(display(",")).toBe(",");
        expect(display("ArrowLeft")).toBe("←");
    });
});

describe("matching a key press", () => {
    it("takes the modifiers as an exact set, not a minimum", () => {
        expect(matchesEvent("ctrl+z", press("z", { ctrl: true }))).toBe(true);
        expect(matchesEvent("ctrl+z", press("z", { ctrl: true, shift: true }))).toBe(false);
        expect(matchesEvent("ctrl+shift+z", press("z", { ctrl: true, shift: true }))).toBe(true);
        expect(matchesEvent("ctrl+shift+z", press("z", { ctrl: true }))).toBe(false);
    });

    it("keeps a bare key clear of the same key under a modifier", () => {
        expect(matchesEvent(",", press(","))).toBe(true);
        expect(matchesEvent(",", press(",", { ctrl: true }))).toBe(false);
        expect(matchesEvent("ctrl+,", press(","))).toBe(false);
    });

    it("ignores the case Shift puts a letter in", () => {
        expect(matchesEvent("ctrl+shift+z", press("Z", { ctrl: true, shift: true }))).toBe(true);
    });
});

describe("the registry", () => {
    it("gives every action a group that exists", () => {
        const groups = new Set<string>(shortcutGroups.map((g) => g.id));
        for (const action of shortcutActions) expect(groups).toContain(action.group);
    });

    it("hands out no default twice", () => {
        const seen = new Set<string>();
        for (const action of shortcutActions) {
            for (const b of action.defaults) {
                expect(seen.has(b)).toBe(false);
                seen.add(b);
            }
        }
    });
});

describe("the shortcut store", () => {
    /** A fresh store over a file holding `contents`. */
    async function storeOver(contents: Record<string, unknown> = {}) {
        const file = fileWith(contents);
        load.mockReset();
        load.mockResolvedValue(file);
        vi.resetModules();
        const { shortcuts } = await import("$lib/settings/shortcuts.svelte");
        await shortcuts.hydrated;
        return { shortcuts, file };
    }

    beforeEach(() => {
        load.mockReset();
    });

    it("answers with the registry's defaults until something is rebound", async () => {
        const { shortcuts } = await storeOver();

        expect(shortcuts.for("undo")).toEqual(["ctrl+z"]);
        expect(shortcuts.isCustom("undo")).toBe(false);
        expect(shortcuts.isAnyCustom).toBe(false);
        expect(shortcuts.display("redo")).toBe("Ctrl+Shift+Z");
    });

    it("lets one action hold several bindings", async () => {
        const { shortcuts } = await storeOver();

        expect(shortcuts.add("undo", "ctrl+u")).toEqual([]);

        expect(shortcuts.for("undo")).toEqual(["ctrl+z", "ctrl+u"]);
        expect(shortcuts.isCustom("undo")).toBe(true);
        expect(shortcuts.actionFor(press("u", { ctrl: true }))).toBe("undo");
        expect(shortcuts.actionFor(press("z", { ctrl: true }))).toBe("undo");
    });

    it("takes a binding off whoever held it, rather than leaving it dead", async () => {
        const { shortcuts } = await storeOver();

        expect(shortcuts.add("redo", "ctrl+z")).toEqual(["Undo"]);

        expect(shortcuts.for("undo")).toEqual([]);
        expect(shortcuts.for("redo")).toEqual(["ctrl+shift+z", "ctrl+z"]);
        expect(shortcuts.actionFor(press("z", { ctrl: true }))).toBe("redo");
    });

    it("says nothing runs on a press nothing is bound to", async () => {
        const { shortcuts } = await storeOver();
        expect(shortcuts.actionFor(press("q", { ctrl: true }))).toBeNull();
    });

    it("stops counting as changed once it is back where it started", async () => {
        const { shortcuts } = await storeOver();

        shortcuts.add("undo", "ctrl+u");
        shortcuts.remove("undo", "ctrl+u");

        expect(shortcuts.for("undo")).toEqual(["ctrl+z"]);
        expect(shortcuts.isCustom("undo")).toBe(false);
    });

    it("puts one action back without touching the others", async () => {
        const { shortcuts } = await storeOver();

        shortcuts.add("undo", "ctrl+u");
        shortcuts.add("openImage", "ctrl+shift+o");
        shortcuts.reset("undo");

        expect(shortcuts.for("undo")).toEqual(["ctrl+z"]);
        expect(shortcuts.for("openImage")).toEqual(["ctrl+o", "ctrl+shift+o"]);
    });

    it("throws every change away on a factory reset", async () => {
        const { shortcuts } = await storeOver();

        shortcuts.add("redo", "ctrl+z");
        shortcuts.add("openImage", "ctrl+shift+o");
        shortcuts.resetAll();

        expect(shortcuts.isAnyCustom).toBe(false);
        expect(shortcuts.for("undo")).toEqual(["ctrl+z"]);
        expect(shortcuts.for("redo")).toEqual(["ctrl+shift+z"]);
    });

    it("records only what differs from the defaults", async () => {
        const { shortcuts, file } = await storeOver();

        shortcuts.add("undo", "ctrl+u");

        await vi.waitFor(() =>
            expect(file.set).toHaveBeenCalledWith("shortcuts", {
                undo: ["ctrl+z", "ctrl+u"],
            }),
        );
        expect(file.save).toHaveBeenCalled();
    });

    it("starts on what the file says rather than the defaults", async () => {
        const { shortcuts } = await storeOver({ shortcuts: { undo: ["ctrl+u"] } });

        expect(shortcuts.for("undo")).toEqual(["ctrl+u"]);
        expect(shortcuts.isCustom("undo")).toBe(true);
        expect(shortcuts.for("redo")).toEqual(["ctrl+shift+z"]);
    });

    it("does not write back what it just read", async () => {
        const { file } = await storeOver({ shortcuts: { undo: ["ctrl+u"] } });
        expect(file.set).not.toHaveBeenCalled();
    });

    it("reads a hand-edited file as far as it makes sense", async () => {
        const { shortcuts } = await storeOver({
            shortcuts: {
                undo: ["ctrl+u", 7, "", "ctrl+u"],
                notAnAction: ["ctrl+k"],
                redo: "ctrl+y",
            },
        });

        expect(shortcuts.for("undo")).toEqual(["ctrl+u"]);
        expect(shortcuts.for("redo")).toEqual(["ctrl+shift+z"]);
    });
});
