// tests/browser/editorShell.browser.test.ts
//
// The whole page, mounted with the Tauri backend stubbed out: what the editor
// lays out in each mode, which parts wait for an image, and what each tool
// hands the backend once it is used.

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { mount, unmount } from "svelte";

import "$lib/styles/tailwind.css";
import "$lib/styles/global.css";
import "$lib/styles/palette.css";
import "$lib/styles/glass.css";

import { initializeGPU } from "$lib/gpu/gpuInit";
import { history } from "$lib/history/history.svelte";
import { presets } from "$lib/presets/presets.svelte";
import { retouchTool } from "$lib/retouch/retouchTool.svelte";
import { FULL_FRAME } from "$lib/types/crop";
import type { GPUSession } from "$lib/types/gpuTypes";

const SIZE = 8;
const HIST_BYTES = 3 * 256 * 4;
const HALF_GREY = 0x3800;
const HALF_ONE = 0x3c00;

function fillPixels(view: DataView, offset: number, count: number) {
    for (let i = 0; i < count; i++) {
        for (let c = 0; c < 4; c++) view.setUint16(offset + (i * 4 + c) * 2, c === 3 ? HALF_ONE : HALF_GREY, true);
    }
}

function openPayload(): ArrayBuffer {
    const pixelBytes = SIZE * SIZE * 8;
    const buffer = new ArrayBuffer(64 + 2 * pixelBytes + HIST_BYTES);
    const view = new DataView(buffer);
    [SIZE, SIZE, SIZE, SIZE, SIZE, SIZE].forEach((v, i) => view.setUint32(i * 4, v, true));
    [1, 0, 0, 0, 1, 0, 0, 0, 1].forEach((v, i) => view.setFloat32(24 + i * 4, v, true));
    fillPixels(view, 64, SIZE * SIZE * 2);
    // A spread of levels in every channel, so auto levels has an edge to find.
    for (let bin = 0; bin < 3 * 256; bin++) {
        if (bin % 256 >= 20 && bin % 256 <= 230) view.setUint32(64 + 2 * pixelBytes + bin * 4, 100, true);
    }
    return buffer;
}

function rowsBand(y: number, rows: number): ArrayBuffer {
    const count = Math.min(rows, SIZE - y);
    const buffer = new ArrayBuffer(16 + SIZE * count * 8);
    const view = new DataView(buffer);
    [SIZE, SIZE, y, count].forEach((v, i) => view.setUint32(i * 4, v, true));
    fillPixels(view, 16, SIZE * count);
    return buffer;
}

// Every export the page asks for, and the text on the stubbed clipboard.
const exports: any[] = [];
let clipboard = "";

function stubTauri() {
    let callback = 0;
    (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
            if (cmd === "open_image_file") return openPayload();
            if (cmd === "full_resolution_rows") return rowsBand(args.y, args.rows);
            if (cmd === "crop_histograms") return new ArrayBuffer(HIST_BYTES);
            if (cmd === "export_image") return void exports.push(JSON.parse(JSON.stringify(args)));
            if (cmd === "plugin:clipboard-manager|write_text") return void (clipboard = args.text);
            if (cmd === "plugin:clipboard-manager|read_text") return clipboard;
            if (/^plugin:store\|(entries|keys|values)$/.test(cmd)) return [];
            if (cmd === "plugin:event|listen") return ++callback;
            return null;
        },
        transformCallback: () => ++callback,
        unregisterCallback: () => {},
        convertFileSrc: (path: string) => path,
        metadata: {
            currentWindow: { label: "main" },
            currentWebview: { windowLabel: "main", label: "main" },
        },
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
}

// ── Page helpers ──────────────────────────────────────────────────────────────

let app: Record<string, any>;
let gpu: GPUSession;

const $ = <T extends Element = HTMLElement>(selector: string) => document.querySelector<T>(selector);
const isDarkroom = () => document.documentElement.dataset.theme === "darkroom";
const button = (label: string) => $<HTMLButtonElement>(`[aria-label="${label}"]`)!;

function pager(): HTMLElement {
    const all = [...document.querySelectorAll<HTMLElement>(".controls-pager")];
    const current = all.find((el) => (el.querySelector("canvas.live-histogram") !== null) === isDarkroom());
    if (!current) throw new Error("no pager for the current mode");
    return current;
}

/** Each page's section titles and control names, in the order they are laid out. */
function layout(): string[][] {
    return [...pager().querySelectorAll<HTMLElement>(".controls-page")].map((p) =>
        [...p.querySelectorAll<HTMLElement>(".section-title, span.name")].map((el) => el.textContent!.trim()),
    );
}

function group(name: string): HTMLElement {
    const label = [...pager().querySelectorAll<HTMLElement>("span.name")].find((el) => el.textContent?.trim() === name);
    const el = label?.closest<HTMLElement>(".flex-col");
    if (!el) throw new Error(`no control named ${name}`);
    return el;
}

const value = (name: string) => Number(group(name).querySelector("[role=slider]")!.getAttribute("aria-valuenow"));

async function nudge(name: string, key = "{ArrowRight}") {
    group(name).querySelector<HTMLElement>("[role=slider]")!.focus();
    await userEvent.keyboard(key);
}

async function goToPage(index: number) {
    pager().scrollTo({ left: index * pager().clientWidth, behavior: "instant" });
    await vi.waitFor(() => expect(Math.round(pager().scrollLeft / pager().clientWidth)).toBe(index));
}

async function setMode(darkroom: boolean) {
    if (isDarkroom() !== darkroom) button("Toggle between lighter-room and darkroom mode").click();
    await vi.waitFor(() => {
        expect(isDarkroom()).toBe(darkroom);
        expect(document.querySelectorAll(".controls-pager")).toHaveLength(1);
    }, { timeout: 5000 });
}

async function exported() {
    const count = exports.length;
    document.querySelector<HTMLButtonElement>(".export-button")!.click();
    await vi.waitFor(() => expect(exports).toHaveLength(count + 1));
    return exports[count];
}

const isInert = (selector: string) => $(selector)!.hasAttribute("inert");

// ── Setup ─────────────────────────────────────────────────────────────────────

beforeAll(async () => {
    stubTauri();
    await page.viewport(1400, 900);
    gpu = await initializeGPU();
    const Page = (await import("../../src/routes/+page.svelte")).default;
    app = mount(Page, { target: document.body, context: new Map([["gpu", () => gpu]]) });
});

afterAll(() => unmount(app));

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("before an image is open", () => {
    it("holds back the tools and the adjustments", () => {
        expect(isInert(".toolbar-tools")).toBe(true);
        expect(isInert(".toolbar-zoom")).toBe(true);
        expect(isInert(".controls-section")).toBe(true);
        expect(isInert(".quick-actions")).toBe(true);
        expect($(".toolbar-tools")!.classList.contains("dimmed")).toBe(true);
    });

    it("opens one from the keyboard", async () => {
        await userEvent.keyboard("{Control>}o{/Control}");
        await vi.waitFor(() => expect(isInert(".controls-section")).toBe(false));
        expect(isInert(".toolbar-tools")).toBe(false);
        expect(isInert(".quick-actions")).toBe(false);
    });
});

describe("with an image open", () => {
    beforeEach(async () => {
        await vi.waitFor(() => expect(isInert(".controls-section")).toBe(false));
        if (retouchTool.mode) retouchTool.mode = null;
        presets.open = false;
        for (const darkroom of [true, false]) {
            await setMode(darkroom);
            await goToPage(1);
            await userEvent.keyboard("{Control>}{Alt>}r{/Alt}{/Control}");
        }
        // A reset travels to its values, and an edit made on the way is overtaken.
        await new Promise((r) => setTimeout(r, 500));
        history.clear();
    });

    it("lays out the lighter-room pages", () => {
        expect(layout()).toEqual([
            ["Sharpness", "Clarity", "Texture", "Unsharp Mask", "Amount", "Radius", "Luma Threshold", "Detail Threshold"],
            [
                "White Balance", "Temperature", "Tint",
                "Light & Colour", "Exposure", "Contrast", "Brightness", "Vibrance", "Saturation",
                "Highlights", "Shadows", "Whites", "Blacks",
            ],
            ["Curves", "HSL", "Hue", "Vibrance", "Saturation", "Colour Mixer", "Hue", "Saturation", "Luminance"],
        ]);
    });

    it("lays out the darkroom pages", async () => {
        await setMode(true);
        expect(layout()).toEqual([
            ["Unsharp Mask", "Amount", "Radius", "Luma Threshold", "Detail Threshold"],
            ["Red Input", "Green Input", "Blue Input", "RGB Input", "RGB Output"],
        ]);
    });

    it("leaves the colour sections out in B&W and brings them back", async () => {
        button("Toggle between RGB and composite color mode").click();
        await vi.waitFor(() => expect(layout()[1]).not.toContain("White Balance"));
        expect(layout()[2]).not.toContain("HSL");
        button("Toggle between RGB and composite color mode").click();
        await vi.waitFor(() => expect(layout()[1]).toContain("White Balance"));
    });

    it("exports the edit as it stands", async () => {
        await nudge("Exposure");
        const exposure = value("Exposure");
        await setMode(true);
        button("Toggle image inversion").click();
        const args = await exported();
        expect(args.sliders.exposure).toBe(exposure);
        expect(args.sliders.exposure).not.toBe(0);
        expect(args.sliders.invert).toBe(true);
        expect(args.crop).toEqual(FULL_FRAME);
        expect(args.retouch).toEqual([]);
        expect(args.settings).toBeTypeOf("object");
        expect(Array.isArray(args.sliders.curveLut)).toBe(true);
        expect(history.stack.map((a) => a.type)).toEqual(["slider", "slider"]);
        button("Toggle image inversion").click();
    });

    it("applies a crop from the crop tool as one history step", async () => {
        await userEvent.keyboard("r");
        await vi.waitFor(() => expect($(".crop-tools")).not.toBeNull());
        expect(isInert(".toolbar-rest")).toBe(true);
        expect(isInert(".controls-section")).toBe(true);
        button("Rotate right").click();
        await userEvent.keyboard("{Enter}");
        await vi.waitFor(() => expect($(".crop-tools")).toBeNull());
        expect(history.stack.map((a) => a.type)).toEqual(["crop"]);
        expect((await exported()).crop.quarterTurns).toBe(1);

        await userEvent.keyboard("{Control>}z{/Control}");
        expect((await exported()).crop.quarterTurns).toBe(0);
    });

    it("throws away an unapplied crop on Escape", async () => {
        await userEvent.keyboard("r");
        await vi.waitFor(() => expect($(".crop-tools")).not.toBeNull());
        button("Flip horizontally").click();
        await userEvent.keyboard("{Escape}");
        await vi.waitFor(() => expect($(".crop-tools")).toBeNull());
        expect(history.stack).toHaveLength(0);
        expect((await exported()).crop.flipX).toBe(false);
    });

    it("opens the presets beside the toolbar and closes them for the crop tool", async () => {
        presets.toggle();
        await vi.waitFor(() => expect($(".presets-column")!.classList.contains("open")).toBe(true));
        expect(isInert(".presets-column")).toBe(false);
        expect($(".chrome-scrim")!.classList.contains("shifted")).toBe(true);
        await userEvent.keyboard("r");
        await vi.waitFor(() => expect(presets.open).toBe(false));
        await userEvent.keyboard("{Escape}");
    });

    it("sends the side bar away while a brush is in hand", async () => {
        button("Heal").click();
        await vi.waitFor(() => expect($(".side-bar")!.classList.contains("away")).toBe(true));
        expect(isInert(".side-bar")).toBe(true);
        await userEvent.keyboard("{Escape}");
        await vi.waitFor(() => expect($(".side-bar")!.classList.contains("away")).toBe(false));
    });

    it("opens and closes the export menu", async () => {
        button("Export settings").click();
        await vi.waitFor(() => expect($('[role=menu][aria-label="Export settings"]')).not.toBeNull());
        expect($(".controls-section")!.classList.contains("dimmed")).toBe(true);
        document.body.click();
        await vi.waitFor(() => expect($('[role=menu][aria-label="Export settings"]')).toBeNull());
    });

    it("reverts the mode's adjustments in one step", async () => {
        await nudge("Exposure");
        await nudge("Contrast");
        const revert = $(".quick-actions .revert-slot")!;
        await vi.waitFor(() => expect(revert.classList.contains("shown")).toBe(true));
        revert.querySelector("button")!.click();
        await expect.poll(() => value("Exposure")).toBe(0);
        expect(value("Contrast")).toBe(0);
        expect(history.stack.map((a) => a.type)).toEqual(["slider", "slider", "sliders"]);
    });

    it("copies and pastes the mode's adjustments", async () => {
        await nudge("Saturation");
        const saturation = value("Saturation");
        await userEvent.keyboard("{Control>}c{/Control}");
        await vi.waitFor(() => expect(clipboard).toContain("saturation"));
        await nudge("Saturation", "{ArrowLeft}{ArrowLeft}");
        await userEvent.keyboard("{Control>}v{/Control}");
        await expect.poll(() => value("Saturation")).toBe(saturation);
    });

    it("sets the darkroom's levels automatically as one step", async () => {
        await setMode(true);
        await goToPage(0);
        button("Auto black and white points").click();
        await vi.waitFor(() => expect(history.stack.map((a) => a.type)).toEqual(["sliders"]));
    });
});
