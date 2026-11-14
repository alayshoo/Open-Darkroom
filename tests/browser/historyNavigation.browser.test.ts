// tests/browser/historyNavigation.browser.test.ts
//
// The whole page, mounted with the Tauri backend stubbed out, driven through
// real sliders and the undo/redo shortcuts: each step brings its mode and its
// lighter-room page into view, and lands the value it recorded.

import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { mount, unmount } from "svelte";

import "$lib/styles/tailwind.css";
import "$lib/styles/global.css";
import "$lib/styles/palette.css";
import "$lib/styles/glass.css";

import { initializeGPU } from "$lib/gpu/gpuInit";
import { history } from "$lib/history/history.svelte";
import type { GPUSession } from "$lib/types/gpuTypes";
import { catalogCommand } from "./catalogStub";

const SIZE = 8;
const HIST_BYTES = 3 * 256 * 4;
const HALF_GREY = 0x3800;
const HALF_ONE = 0x3c00;

function fillPixels(view: DataView, offset: number, count: number) {
    for (let i = 0; i < count; i++) {
        for (let c = 0; c < 4; c++) view.setUint16(offset + (i * 4 + c) * 2, c === 3 ? HALF_ONE : HALF_GREY, true);
    }
}

/** An open payload for a flat grey SIZE×SIZE image, laid out as image_opening.rs writes it. */
function openPayload(): ArrayBuffer {
    const pixelBytes = SIZE * SIZE * 8;
    const buffer = new ArrayBuffer(64 + 2 * pixelBytes + HIST_BYTES);
    const view = new DataView(buffer);
    [SIZE, SIZE, SIZE, SIZE, SIZE, SIZE].forEach((v, i) => view.setUint32(i * 4, v, true));
    [1, 0, 0, 0, 1, 0, 0, 0, 1].forEach((v, i) => view.setFloat32(24 + i * 4, v, true));
    fillPixels(view, 64, SIZE * SIZE * 2);
    return buffer;
}

/** A band of full-resolution rows, laid out as full_resolution.rs writes it. */
function rowsBand(y: number, rows: number): ArrayBuffer {
    const count = Math.min(rows, SIZE - y);
    const buffer = new ArrayBuffer(16 + SIZE * count * 8);
    const view = new DataView(buffer);
    [SIZE, SIZE, y, count].forEach((v, i) => view.setUint32(i * 4, v, true));
    fillPixels(view, 16, SIZE * count);
    return buffer;
}

function stubTauri() {
    let callback = 0;
    (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
            const catalog = catalogCommand(cmd, openPayload);
            if (catalog !== undefined) return catalog;
            if (cmd === "full_resolution_rows") return rowsBand(args.y, args.rows);
            if (cmd === "crop_histograms") return new ArrayBuffer(HIST_BYTES);
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

const isDarkroom = () => document.documentElement.dataset.theme === "darkroom";

/** The pager on screen, and the page it is showing. */
function pager(): HTMLElement {
    const all = [...document.querySelectorAll<HTMLElement>(".controls-pager")];
    const current = all.find((el) => (el.querySelector("canvas.live-histogram") !== null) === isDarkroom());
    if (!current) throw new Error("no pager for the current mode");
    return current;
}
const pageIndex = () => Math.round(pager().scrollLeft / pager().clientWidth);

/** The control group carrying `name` as its label, in the current mode's pager. */
function group(name: string): HTMLElement {
    const label = [...pager().querySelectorAll<HTMLElement>("span.name")].find((el) => el.textContent?.trim() === name);
    const el = label?.closest<HTMLElement>(".flex-col");
    if (!el) throw new Error(`no control named ${name}`);
    return el;
}

/** The value of one handle of a group, or of its only slider. */
function value(name: string, handle?: "A" | "B" | "C"): number {
    const el = group(name).querySelector<HTMLElement>(handle ? `[data-pointer="${handle}"]` : "[role=slider]")!;
    return Number(el.getAttribute("aria-valuenow"));
}

/** Drag a single-value slider from 30% to 70% of its track. */
function dragSlider(name: string) {
    const track = group(name).querySelector<HTMLElement>("[role=slider]")!;
    const box = track.getBoundingClientRect();
    const y = box.top + box.height / 2;
    const at = (f: number) => ({ clientX: box.left + box.width * f, clientY: y, button: 0, bubbles: true, pointerId: 1 });
    track.dispatchEvent(new PointerEvent("pointerdown", at(0.3)));
    document.dispatchEvent(new PointerEvent("pointermove", at(0.5)));
    document.dispatchEvent(new PointerEvent("pointermove", at(0.7)));
    document.dispatchEvent(new PointerEvent("pointerup", at(0.7)));
}

/** Nudge one handle of a darkroom group with the keyboard, away from the rail it is nearest. */
async function nudgeHandle(name: string, handle: "A" | "B" | "C") {
    const el = group(name).querySelector<HTMLElement>(`[data-pointer="${handle}"]`)!;
    const [now, min, max] = ["now", "min", "max"].map((k) => Number(el.getAttribute(`aria-value${k}`)));
    el.focus();
    await userEvent.keyboard(now - min > max - now ? "{ArrowLeft}" : "{ArrowRight}");
}

async function goToPage(index: number) {
    pager().scrollTo({ left: index * pager().clientWidth, behavior: "instant" });
    await vi.waitFor(() => expect(pageIndex()).toBe(index));
}

async function setMode(darkroom: boolean) {
    if (isDarkroom() !== darkroom) {
        document.querySelector<HTMLElement>('[aria-label="Toggle between lighter-room and darkroom mode"]')!.click();
    }
    await vi.waitFor(() => {
        expect(isDarkroom()).toBe(darkroom);
        expect(document.querySelectorAll(".controls-pager")).toHaveLength(1);
    }, { timeout: 5000 });
}

const undo = () => userEvent.keyboard("{Control>}z{/Control}");
const redo = () => userEvent.keyboard("{Control>}{Shift>}z{/Shift}{/Control}");

// ── Setup ─────────────────────────────────────────────────────────────────────

beforeAll(async () => {
    stubTauri();
    await page.viewport(1400, 900);
    gpu = await initializeGPU();
    const Page = (await import("../../src/routes/+page.svelte")).default;
    app = mount(Page, { target: document.body, context: new Map([["gpu", () => gpu]]) });

    await userEvent.keyboard("{Control>}o{/Control}");
    await vi.waitFor(() => expect(document.querySelector(".controls-section")?.hasAttribute("inert")).toBe(false));
});

afterAll(() => unmount(app));

// Each mode opens on its middle page, so the darkroom starts on the input levels.
beforeEach(async () => {
    history.clear();
    await setMode(true);
    await goToPage(1);
    await setMode(false);
    await goToPage(1);
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("undo and redo bring the edited controls into view", () => {
    it("records a darkroom gamma edit and undoes it", async () => {
        await setMode(true);
        const before = value("Green Input", "B");
        await nudgeHandle("Green Input", "B");
        const after = value("Green Input", "B");
        expect(after).not.toBe(before);
        expect(history.stack).toHaveLength(1);

        await undo();
        await expect.poll(() => value("Green Input", "B")).toBe(before);
        await redo();
        await expect.poll(() => value("Green Input", "B")).toBe(after);
    });

    it("records every handle of every darkroom input group", async () => {
        await setMode(true);
        const groups = ["Red Input", "Green Input", "Blue Input", "RGB Input"];
        for (const name of groups) {
            await nudgeHandle(name, "A");
            await nudgeHandle(name, "B");
            await nudgeHandle(name, "C");
        }
        await nudgeHandle("RGB Output", "A");
        await nudgeHandle("RGB Output", "B");
        expect(history.stack.map((a) => a.type === "slider" && a.key)).toEqual([
            "redBlackPoint", "redGamma", "redWhitePoint",
            "greenBlackPoint", "greenGamma", "greenWhitePoint",
            "blueBlackPoint", "blueGamma", "blueWhitePoint",
            "masterBlackPoint", "masterGamma", "masterWhitePoint",
            "rgbOutputBlack", "rgbOutputWhite",
        ]);
    });

    it("switches to the darkroom to undo a darkroom edit", async () => {
        await setMode(true);
        const before = value("Red Input", "B");
        await nudgeHandle("Red Input", "B");
        await setMode(false);

        await undo();
        await expect.poll(isDarkroom).toBe(true);
        await expect.poll(() => value("Red Input", "B")).toBe(before);
    });

    it("switches to lighter-room on the edit's page to undo it from the darkroom", async () => {
        await goToPage(0);
        dragSlider("Clarity");
        await goToPage(1);
        await setMode(true);

        await undo();
        await expect.poll(isDarkroom).toBe(false);
        await expect.poll(pageIndex).toBe(0);
    });

    it("jumps to the page holding the edit without scrolling to it", async () => {
        dragSlider("Exposure");
        await goToPage(0);

        await undo();
        expect(pageIndex()).toBe(1);
    });

    it("stays on the page when it already shows the edited control", async () => {
        dragSlider("Saturation");
        await goToPage(2);

        await undo();
        await redo();
        expect(pageIndex()).toBe(2);
    });

    it("records arrow-key edits on a lighter-room slider", async () => {
        const before = value("Exposure");
        group("Exposure").querySelector<HTMLElement>("[role=slider]")!.focus();
        await userEvent.keyboard("{ArrowRight}");
        expect(value("Exposure")).not.toBe(before);
        expect(history.stack).toHaveLength(1);

        await undo();
        await expect.poll(() => value("Exposure")).toBe(before);
    });

    it("jumps to the darkroom sharpness page to undo an unsharp mask edit there", async () => {
        await setMode(true);
        await goToPage(0);
        const before = value("Radius");
        dragSlider("Radius");
        await goToPage(1);

        await undo();
        expect(isDarkroom()).toBe(true);
        expect(pageIndex()).toBe(0);
        await expect.poll(() => value("Radius")).toBe(before);
    });

    it("undoes an unsharp mask edit in the mode the user is in", async () => {
        await setMode(true);
        await goToPage(0);
        dragSlider("Amount");
        await setMode(false);

        await undo();
        await new Promise((r) => setTimeout(r, 600));
        expect(isDarkroom()).toBe(false);
        expect(pageIndex()).toBe(0);
    });

    it("brings redo into view the same way", async () => {
        await setMode(true);
        await nudgeHandle("Blue Input", "C");
        await setMode(false);
        await goToPage(0);
        dragSlider("Texture");

        await undo();
        await undo();
        await expect.poll(isDarkroom).toBe(true);

        await redo();
        await expect.poll(isDarkroom).toBe(true);
        await redo();
        await expect.poll(isDarkroom).toBe(false);
        await expect.poll(pageIndex).toBe(0);
    });
});
