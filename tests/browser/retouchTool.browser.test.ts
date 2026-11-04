// tests/browser/retouchTool.browser.test.ts
//
// The heal and clone brushes on the whole page, with the Tauri backend stubbed
// out: taking a brush up, painting a stroke onto the preview, Alt-clicking a source,
// sizing the brush, and the strokes landing in the history.

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { mount, unmount } from "svelte";

import "$lib/styles/tailwind.css";
import "$lib/styles/global.css";
import "$lib/styles/palette.css";
import "$lib/styles/glass.css";

import { initializeGPU } from "$lib/gpu/gpuInit";
import { history } from "$lib/history/history.svelte";
import { retouchTool } from "$lib/retouch/retouchTool.svelte";
import type { GPUSession } from "$lib/types/gpuTypes";

const SIZE = 256;
const HIST_BYTES = 3 * 256 * 4;
const HALF_GREY = 0x3800;
const HALF_ONE = 0x3c00;

/** A flat grey SIZE × SIZE open payload, laid out as image_opening.rs writes it. */
function openPayload(): ArrayBuffer {
    const pixelBytes = SIZE * SIZE * 8;
    const buffer = new ArrayBuffer(64 + 2 * pixelBytes + HIST_BYTES);
    const view = new DataView(buffer);
    [SIZE, SIZE, SIZE, SIZE, SIZE, SIZE].forEach((v, i) => view.setUint32(i * 4, v, true));
    [1, 0, 0, 0, 1, 0, 0, 0, 1].forEach((v, i) => view.setFloat32(24 + i * 4, v, true));
    for (let i = 0; i < SIZE * SIZE * 2; i++) {
        for (let c = 0; c < 4; c++) view.setUint16(64 + (i * 4 + c) * 2, c === 3 ? HALF_ONE : HALF_GREY, true);
    }
    return buffer;
}

/** How many strokes each request for the input histograms carried. */
const binned: number[] = [];

function stubTauri() {
    let callback = 0;
    (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args?: { retouch?: unknown[] }) => {
            if (cmd === "open_image_file") return openPayload();
            if (cmd === "crop_histograms") {
                binned.push(args?.retouch?.length ?? 0);
                return new ArrayBuffer(HIST_BYTES);
            }
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

let app: Record<string, any>;
let gpu: GPUSession;

const canvas = () => document.querySelector<HTMLCanvasElement>(".preview-frame canvas")!;
const frame = () => document.querySelector<HTMLElement>(".preview-frame")!.getBoundingClientRect();
const button = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

/** A point on the image, as fractions of the frame. */
function on(fx: number, fy: number) {
    const f = frame();
    return { clientX: f.left + f.width * fx, clientY: f.top + f.height * fy };
}

function pointer(type: string, at: { clientX: number; clientY: number }, keys: PointerEventInit = {}) {
    canvas().dispatchEvent(new PointerEvent(type, { ...at, button: 0, buttons: 1, bubbles: true, pointerId: 1, ...keys }));
}

function paint(points: [number, number][], keys: PointerEventInit = {}) {
    pointer("pointerdown", on(...points[0]), keys);
    for (const p of points.slice(1)) pointer("pointermove", on(...p), keys);
    pointer("pointerup", on(...points[points.length - 1]), keys);
}

beforeAll(async () => {
    stubTauri();
    await page.viewport(1400, 900);
    gpu = await initializeGPU();
    const Page = (await import("../../src/routes/+page.svelte")).default;
    app = mount(Page, { target: document.body, context: new Map([["gpu", () => gpu]]) });
    await userEvent.keyboard("{Control>}o{/Control}");
    await vi.waitFor(() => expect(button("Heal").disabled).toBe(false));
});

afterAll(() => unmount(app));

beforeEach(() => {
    retouchTool.mode = null;
    retouchTool.resetSource();
    retouchTool.radius = 12;
});

describe("the retouch brushes", () => {
    it("are taken up from the toolbar and by shortcut, one at a time", async () => {
        button("Heal").click();
        // The side bar steps out of the way, and the brush's controls open along the foot.
        await vi.waitFor(() => expect(document.querySelector(".retouch-bar.open")).not.toBeNull());
        expect(document.querySelector(".retouch-bar .title")!.textContent).toBe("Heal");
        expect(document.querySelector(".presets-column.open")).toBeNull();
        const sideBar = document.querySelector<HTMLElement>(".side-bar")!;
        expect(sideBar.classList.contains("away")).toBe(true);
        expect(sideBar.inert).toBe(true);
        expect(button("Heal").getAttribute("aria-pressed")).toBe("true");
        await userEvent.keyboard("c");
        expect(retouchTool.mode).toBe("clone");
        expect(document.querySelector(".retouch-bar .title")!.textContent).toBe("Clone");
        await userEvent.keyboard("{Escape}");
        expect(retouchTool.mode).toBeNull();
        expect(document.querySelector(".side-bar")!.classList.contains("away")).toBe(false);
        expect(document.querySelector<HTMLElement>(".retouch-bar")!.inert).toBe(true);
    });

    it("is put down when the presets open", async () => {
        await userEvent.keyboard("x");
        expect(retouchTool.mode).toBe("heal");
        button("Presets").click();
        await vi.waitFor(() => expect(retouchTool.mode).toBeNull());
        expect(document.querySelector(".presets-column.open")).not.toBeNull();
        expect(document.querySelector(".retouch-bar.open")).toBeNull();
        button("Hide presets").click();
    });

    it("draws the stroke and the brush while it is painted", async () => {
        await userEvent.keyboard("x");
        pointer("pointerdown", on(0.4, 0.5));
        pointer("pointermove", on(0.45, 0.5));
        await vi.waitFor(() => {
            expect(document.querySelector(".retouch-brush path.stroke")).not.toBeNull();
            expect(document.querySelector(".retouch-brush circle.ring")).not.toBeNull();
        });
        await userEvent.keyboard("{Escape}");
        pointer("pointerup", on(0.45, 0.5));
        expect(document.querySelector(".retouch-brush path.stroke")).toBeNull();
        expect(retouchTool.mode).toBe("heal");
    });

    it("takes its pixels from a picked source, and keeps the offset for the next stroke", async () => {
        history.clear();
        await userEvent.keyboard("c");
        pointer("pointerdown", on(0.2, 0.2), { altKey: true });
        pointer("pointerup", on(0.2, 0.2), { altKey: true });
        expect(retouchTool.pickedSource).not.toBeNull();
        paint([[0.6, 0.6]]);
        paint([[0.7, 0.3]]);
        await vi.waitFor(() => expect(history.stack).toHaveLength(2));
        const offsets = history.stack.map((a) => (a.type === "retouchAdd" ? a.stroke.offset : null));
        expect(offsets[0]![0]).toBeCloseTo(-0.4, 2);
        expect(offsets[0]![1]).toBeCloseTo(-0.4, 2);
        expect(offsets[1]).toEqual(offsets[0]);

        // With its source known, a stroke shows in the image as it is painted, not as an outline.
        pointer("pointerdown", on(0.6, 0.3));
        pointer("pointermove", on(0.65, 0.3));
        await vi.waitFor(() => expect(document.querySelector(".retouch-brush circle.ring")).not.toBeNull());
        expect(document.querySelector(".retouch-brush path.stroke")).toBeNull();
        await userEvent.keyboard("{Escape}");
        pointer("pointerup", on(0.65, 0.3));
        expect(history.stack).toHaveLength(2);

        await userEvent.keyboard("{Control>}z{/Control}");
        expect(history.pointer).toBe(0);
    });

    it("picks no source from an Alt-click off the image", async () => {
        await userEvent.keyboard("c");
        pointer("pointerdown", on(-0.1, 0.5), { altKey: true });
        pointer("pointerup", on(-0.1, 0.5), { altKey: true });
        expect(retouchTool.pickedSource).toBeNull();
        expect(retouchTool.hasSource).toBe(false);
    });

    it("sizes the brush across an Alt + drag and softens it upwards", async () => {
        await userEvent.keyboard("x");
        const { radius, feather } = retouchTool;
        const start = on(0.5, 0.5);
        pointer("pointerdown", start, { altKey: true });
        pointer("pointermove", { clientX: start.clientX + 40, clientY: start.clientY - 20 }, { altKey: true });
        pointer("pointerup", { clientX: start.clientX + 40, clientY: start.clientY - 20 }, { altKey: true });
        expect(retouchTool.radius).toBeGreaterThan(radius);
        expect(retouchTool.feather).toBeGreaterThan(feather);
        // A drag sizes the brush rather than picking a source.
        expect(retouchTool.pickedSource).toBeNull();
        expect(history.stack.filter((a) => a.type === "retouchAdd")).toHaveLength(history.stack.length);
    });

    it("keeps the keyboard to the brushes and undo, and bins the histograms once put down", async () => {
        history.clear();
        await userEvent.keyboard("c");
        paint([[0.2, 0.2]], { altKey: true });
        const asked = binned.length;
        paint([[0.6, 0.6]]);
        await vi.waitFor(() => expect(history.stack).toHaveLength(1));
        await userEvent.keyboard("r");
        expect(button("Apply crop")).toBeNull();
        expect(retouchTool.mode).toBe("clone");
        expect(binned.length).toBe(asked);

        await userEvent.keyboard("{Escape}");
        await vi.waitFor(() => expect(binned.length).toBe(asked + 1));
        expect(binned.at(-1)).toBeGreaterThan(0);
    });

    it("clears every stroke as one step", async () => {
        history.clear();
        await userEvent.keyboard("c");
        paint([[0.2, 0.2]], { altKey: true });
        paint([[0.6, 0.6]]);
        await vi.waitFor(() => expect(history.stack).toHaveLength(1));
        button("Auto source").click();
        expect(retouchTool.hasSource).toBe(false);
        button("Clear all").click();
        expect(history.stack.map((a) => a.type)).toEqual(["retouchAdd", "retouchClear"]);
    });
});
