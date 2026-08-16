// tests/unit/settingsPersistence.test.ts
//
// Preferences are meant to survive the window closing, so what matters is the
// pair: what a save writes to the OS user's file, and what the next launch
// reads back out of it. The store plugin is mocked with a map, which is enough
// to pin both halves and the leniency the reader promises a hand-edited file.

import { beforeEach, describe, expect, it, vi } from "vitest";

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

/**
 * A fresh copy of the module. It holds the opened file for the life of the
 * process, so tests that want a different one need their own import.
 */
async function persistence() {
    vi.resetModules();
    return import("$lib/settings/persistence");
}

beforeEach(() => {
    load.mockReset();
});

describe("the settings file", () => {
    it("is opened by name, once, however many calls are made", async () => {
        load.mockResolvedValue(fileWith());
        const { loadSettings, saveSetting, SETTINGS_FILE } = await persistence();

        await loadSettings();
        await saveSetting("debugTools", true);
        await loadSettings();

        expect(SETTINGS_FILE).toBe("settings.json");
        expect(load).toHaveBeenCalledTimes(1);
        expect(load).toHaveBeenCalledWith("settings.json");
    });

    it("is written and flushed by a save, not left pending", async () => {
        const file = fileWith();
        load.mockResolvedValue(file);
        const { saveSetting } = await persistence();

        await saveSetting("debugTools", false);

        expect(file.set).toHaveBeenCalledWith("debugTools", false);
        expect(file.save).toHaveBeenCalled();
        expect(file.data.get("debugTools")).toBe(false);
    });

    it("gives back what was written to it", async () => {
        const file = fileWith();
        load.mockResolvedValue(file);
        const { loadSettings, saveSetting } = await persistence();

        await saveSetting("debugTools", false);
        expect(await loadSettings()).toEqual({ debugTools: false });

        await saveSetting("debugTools", true);
        expect(await loadSettings()).toEqual({ debugTools: true });
    });
});

describe("reading a file", () => {
    /** What loadSettings makes of a file holding exactly `contents`. */
    async function read(contents: Record<string, unknown>) {
        load.mockResolvedValue(fileWith(contents));
        const { loadSettings } = await persistence();
        return loadSettings();
    }

    it("says nothing about a preference the file does not mention", async () => {
        expect(await read({})).toEqual({});
    });

    it("ignores keys that are not preferences", async () => {
        expect(await read({ debugTools: true, windowWidth: 900 })).toEqual({
            debugTools: true,
        });
    });

    it("drops a value of the wrong type rather than the whole file", async () => {
        expect(await read({ debugTools: "yes" })).toEqual({});
        expect(await read({ debugTools: 1 })).toEqual({});
        expect(await read({ debugTools: null })).toEqual({});
    });
});

describe("with no backend to reach", () => {
    beforeEach(() => {
        vi.spyOn(console, "warn").mockImplementation(() => {});
    });

    it("falls back to the defaults instead of throwing", async () => {
        load.mockRejectedValue(new Error("no IPC"));
        const { loadSettings } = await persistence();

        await expect(loadSettings()).resolves.toEqual({});
    });

    it("lets a save fail quietly", async () => {
        load.mockRejectedValue(new Error("no IPC"));
        const { saveSetting } = await persistence();

        await expect(saveSetting("debugTools", true)).resolves.toBeUndefined();
    });

    it("tries again on the next call rather than staying broken", async () => {
        const file = fileWith({ debugTools: false });
        load.mockRejectedValueOnce(new Error("no IPC")).mockResolvedValue(file);
        const { loadSettings } = await persistence();

        expect(await loadSettings()).toEqual({});
        expect(await loadSettings()).toEqual({ debugTools: false });
    });
});

describe("the settings store", () => {
    /** A fresh store over a file holding `contents`. */
    async function storeOver(contents: Record<string, unknown>) {
        const file = fileWith(contents);
        load.mockResolvedValue(file);
        vi.resetModules();
        const { settings } = await import("$lib/settings/settings.svelte");
        await settings.hydrated;
        return { settings, file };
    }

    it("starts on the stored value rather than the default", async () => {
        const { settings } = await storeOver({ debugTools: false });
        expect(settings.debugTools).toBe(false);
    });

    it("keeps the default when the file has nothing to say", async () => {
        const { DEFAULTS } = await import("$lib/settings/persistence");
        const { settings } = await storeOver({});
        expect(settings.debugTools).toBe(DEFAULTS.debugTools);
    });

    it("does not write back what it just read", async () => {
        const { file } = await storeOver({ debugTools: false });
        expect(file.set).not.toHaveBeenCalled();
    });

    it("records a preference the moment it is changed", async () => {
        const { settings, file } = await storeOver({ debugTools: false });

        settings.debugTools = true;

        expect(settings.debugTools).toBe(true);
        await vi.waitFor(() =>
            expect(file.set).toHaveBeenCalledWith("debugTools", true),
        );
        expect(file.save).toHaveBeenCalled();
    });

    it("leaves the modal's own state out of the file", async () => {
        const { settings, file } = await storeOver({});

        settings.show("debug");
        settings.close();

        expect(file.set).not.toHaveBeenCalled();
    });
});
