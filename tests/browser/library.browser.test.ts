// tests/browser/library.browser.test.ts
//
// The library over a stubbed catalog: what it lists, opening an image into the
// editor with its stored edit, saving edits as they are made, the way back,
// and the roll and image actions it hands the backend.

import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { page, userEvent } from "vitest/browser";
import { mount, unmount } from "svelte";

import "$lib/styles/tailwind.css";
import "$lib/styles/global.css";
import "$lib/styles/palette.css";
import "$lib/styles/glass.css";

import { initializeGPU } from "$lib/gpu/gpuInit";
import { editor } from "$lib/editor/editor.svelte";
import { library } from "$lib/library/library.svelte";
import { newImageEdit, parseImageEdit, serializeImageEdit } from "$lib/types/imageEdit";
import type { GPUSession } from "$lib/types/gpuTypes";

const SIZE = 8;
const HIST_BYTES = 3 * 256 * 4;

function fillPixels(view: DataView, offset: number, count: number) {
    for (let i = 0; i < count * 4; i++) view.setUint16(offset + i * 2, i % 4 === 3 ? 0x3c00 : 0x3800, true);
}

function openPayload(): ArrayBuffer {
    const pixelBytes = SIZE * SIZE * 8;
    const buffer = new ArrayBuffer(64 + 2 * pixelBytes + HIST_BYTES);
    const view = new DataView(buffer);
    [SIZE, SIZE, SIZE, SIZE, SIZE, SIZE].forEach((v, i) => view.setUint32(i * 4, v, true));
    [1, 0, 0, 0, 1, 0, 0, 0, 1].forEach((v, i) => view.setFloat32(24 + i * 4, v, true));
    fillPixels(view, 64, SIZE * SIZE * 2);
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

const meta = (fileName: string) => ({
    fileName, make: null, model: null, lens: null, focalLength: null,
    aperture: 8, exposureTime: [1, 125], iso: null, exposureBias: null, dateTaken: null,
});

const record = (id: number, rollId: number | null, frame: number | null, fileName: string) => ({
    id, libraryId: 1, rollId, frame, fileName, path: `/scans/${fileName}`, inPhotoFolder: false,
    missing: false, metadata: meta(fileName), frameInfo: {}, edited: null, added: id,
});

// The stored edit for image 2: exposure up a stop.
const STORED = (() => {
    const edit = newImageEdit(SIZE, SIZE);
    edit.sliders.exposure = 1;
    return serializeImageEdit(edit);
})();

const calls: { cmd: string; args: any }[] = [];
const called = (cmd: string) => calls.filter((c) => c.cmd === cmd);

function stubTauri() {
    let callback = 0;
    const catalog = {
        libraries: [{ id: 1, name: "Personal", created: 1 }, { id: 2, name: "Client work", created: 2 }],
        rolls: [{ id: 10, libraryId: 1, name: "Portra in Lisbon", created: 3, info: { filmStock: "Kodak Portra 400" } }],
        images: [record(1, null, null, "loose.jpg"), record(2, 10, 1, "frame01.tif"), record(3, 10, 2, "frame02.tif")],
        photoFolder: null,
    };
    (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
            if (cmd.startsWith("catalog_") || cmd === "open_image_path") calls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? {})) });
            if (cmd === "catalog_load") return structuredClone(catalog);
            if (cmd === "catalog_edit") return args.id === 2 ? STORED : null;
            if (cmd === "catalog_save_edit") return 1234;
            if (cmd === "catalog_thumbnail" || cmd === "catalog_render_thumbnail") return new ArrayBuffer(0);
            if (cmd === "open_image_path") return openPayload();
            if (cmd === "full_resolution_rows") return rowsBand(args.y, args.rows);
            if (cmd === "crop_histograms") return new ArrayBuffer(HIST_BYTES);
            if (/^plugin:store\|(entries|keys|values)$/.test(cmd)) return [];
            if (cmd === "plugin:event|listen") return ++callback;
            return null;
        },
        transformCallback: () => ++callback,
        unregisterCallback: () => {},
        convertFileSrc: (path: string) => path,
        metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
}

let app: Record<string, any>;
let gpu: GPUSession;

const $ = <T extends Element = HTMLElement>(selector: string) => document.querySelector<T>(selector);
const cell = (name: string) => $(`[role=option][aria-label="${name}"]`);
const names = () => [...document.querySelectorAll("[role=option]")].map((el) => el.getAttribute("aria-label"));
const editing = () => library.showing === "editor";

beforeAll(async () => {
    stubTauri();
    await page.viewport(1400, 900);
    gpu = await initializeGPU();
    const Page = (await import("../../src/routes/+page.svelte")).default;
    app = mount(Page, { target: document.body, context: new Map([["gpu", () => gpu]]) });
    await vi.waitFor(() => expect(library.loaded).toBe(true));
});

afterAll(() => unmount(app));

describe("the library", () => {
    it("starts on the first library, with its rolls and loose images", async () => {
        expect(editing()).toBe(false);
        await vi.waitFor(() => expect($("h1")?.textContent).toBe("Personal"));
        expect([...document.querySelectorAll(".library-nav .name")].map((n) => n.textContent)).toEqual([
            "Personal",
            "Portra in Lisbon",
            "Client work",
        ]);
        expect($(".roll-card .summary")!.textContent).toBe("Kodak Portra 400");
        expect(names()).toEqual(["loose.jpg"]);
    });

    it("shows a roll's frames in order, with the frame's logged and recorded settings", async () => {
        $<HTMLButtonElement>(".roll-card")!.click();
        await vi.waitFor(() => expect(names()).toEqual(["frame01.tif", "frame02.tif"]));
        expect($("h1")!.textContent).toBe("Portra in Lisbon");

        cell("frame02.tif")!.click();
        await vi.waitFor(() => expect($(".details h2")?.textContent).toBe("Frame 2"));
        const aperture = $<HTMLInputElement>('.details label:has(.label) input[placeholder="f/8"]');
        expect(aperture).not.toBeNull();
    });

    it("saves what is typed into the roll's information", async () => {
        const stock = [...document.querySelectorAll<HTMLLabelElement>(".details .field")].find(
            (f) => f.querySelector(".label")?.textContent === "Film stock",
        )!;
        const input = stock.querySelector("input")!;
        input.focus();
        input.select();
        await userEvent.keyboard("Kodak Portra 800");
        await vi.waitFor(() => {
            const last = called("catalog_update_roll").at(-1);
            expect(last?.args).toEqual({ id: 10, name: "Portra in Lisbon", info: { filmStock: "Kodak Portra 800" } });
        });
        input.blur();
    });
});

describe("opening an image", () => {
    it("opens it in the editor with the edit stored for it", async () => {
        cell("frame01.tif")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
        await vi.waitFor(() => expect(editing()).toBe(true), { timeout: 5000 });
        await vi.waitFor(() => expect(editor.libraryImageId).toBe(2));
        expect(called("open_image_path").at(-1)!.args.path).toBe("/scans/frame01.tif");
        expect(editor.edit.sliders.exposure).toBe(1);
        await vi.waitFor(() => expect(library.flyingId).toBeNull(), { timeout: 5000 });
        expect(document.querySelector(".library-flyer")).toBeNull();
    });

    it("saves the edit as it changes, once it settles", async () => {
        const before = called("catalog_save_edit").length;
        editor.edit.sliders.contrast = 20;
        editor.edit.sliders.contrast = 25;
        await vi.waitFor(() => expect(called("catalog_save_edit").length).toBe(before + 1));
        const saved = called("catalog_save_edit").at(-1)!.args;
        expect(saved.id).toBe(2);
        expect(parseImageEdit(saved.edit)!.sliders).toMatchObject({ exposure: 1, contrast: 25 });
        await vi.waitFor(() => expect(library.images.find((i) => i.id === 2)!.edited).toBe(1234));
    });

    it("goes back with the arrow above the crop tool, and in again without reopening", async () => {
        const arrow = $<HTMLButtonElement>('.toolbar [aria-label="Back to library"]')!;
        const crop = $<HTMLButtonElement>('.toolbar [aria-label="Crop"]')!;
        expect(arrow.getBoundingClientRect().bottom).toBeLessThan(crop.getBoundingClientRect().top);

        arrow.click();
        await vi.waitFor(() => expect(editing()).toBe(false), { timeout: 5000 });
        await vi.waitFor(() => expect(library.flyingId).toBeNull(), { timeout: 5000 });
        expect(library.view).toEqual({ kind: "roll", id: 10 });
        expect(library.selection).toEqual([2]);
        // Leaving renders the thumbnail with the edit as it stands.
        const render = called("catalog_render_thumbnail").at(-1)!.args;
        expect(render.id).toBe(2);
        expect(render.sliders.contrast).toBe(25);

        const opens = called("open_image_path").length;
        await userEvent.keyboard("g");
        await vi.waitFor(() => expect(editing()).toBe(true), { timeout: 5000 });
        await vi.waitFor(() => expect(library.flyingId).toBeNull(), { timeout: 5000 });
        expect(called("open_image_path")).toHaveLength(opens);
        expect(editor.edit.sliders.contrast).toBe(25);

        await userEvent.keyboard("g");
        await vi.waitFor(() => expect(editing()).toBe(false), { timeout: 5000 });
    });
});

describe("moving and removing", () => {
    it("moves the selected frames to another library from the context menu", async () => {
        await vi.waitFor(() => expect(library.flyingId).toBeNull(), { timeout: 5000 });
        library.selection = [2, 3];
        cell("frame02.tif")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 400, clientY: 300 }));
        const target = await vi.waitFor(() => {
            const item = [...document.querySelectorAll<HTMLButtonElement>(".context-menu .item")].find(
                (b) => b.textContent?.trim() === "Client work",
            );
            expect(item).toBeDefined();
            return item!;
        });
        target.click();
        await vi.waitFor(() => expect(called("catalog_move_images").length).toBe(1));
        expect(called("catalog_move_images")[0].args).toEqual({ ids: [2, 3], place: { libraryId: 2, rollId: null } });
    });

    it("asks before removing images from the catalog", async () => {
        library.show({ kind: "library", id: 1 });
        await vi.waitFor(() => expect(cell("loose.jpg")).not.toBeNull());
        cell("loose.jpg")!.click();
        await userEvent.keyboard("{Delete}");
        await vi.waitFor(() => expect($(".notice")?.textContent).toContain("Remove this image from the catalog"));
        expect(called("catalog_remove_images")).toHaveLength(0);

        [...document.querySelectorAll<HTMLButtonElement>(".notice button")].find((b) => b.textContent === "Remove")!.click();
        await vi.waitFor(() => expect(called("catalog_remove_images")[0]?.args).toEqual({ ids: [1] }));
        await vi.waitFor(() => expect(cell("loose.jpg")).toBeNull());
    });
});
