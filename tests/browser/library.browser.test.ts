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
import { settings } from "$lib/settings/settings.svelte";
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

const gearRecord = (id: number, kind: string, maker: string, name: string, details: Record<string, string>) => ({
    id, kind, maker, name, mount: "", formats: [], process: "", ...details,
});

let nextGear = 10;

const calls: { cmd: string; args: any }[] = [];
const called = (cmd: string) => calls.filter((c) => c.cmd === cmd);

function stubTauri() {
    let callback = 0;
    const catalog = {
        libraries: [{ id: 1, name: "Personal", created: 1 }, { id: 2, name: "Client work", created: 2 }],
        rolls: [{ id: 10, libraryId: 1, name: "Portra in Lisbon", created: 3, stockId: 1, info: { filmStock: "Kodak Portra 400" } }],
        images: [record(1, null, null, "loose.jpg"), record(2, 10, 1, "frame01.tif"), record(3, 10, 2, "frame02.tif")],
        filmStocks: [
            { id: 1, maker: "Kodak", name: "Portra 400", boxIso: 400, kind: "colourNegative", process: "C-41", formats: ["35mm", "120"], colour: "#f2b33d", colour2: "#e4572e" },
            { id: 2, maker: "Kodak", name: "Tri-X 400", boxIso: 400, kind: "blackAndWhite", process: "B&W", formats: ["35mm"], colour: "#f7c600", colour2: "#2b2b2b" },
        ],
        gear: [
            gearRecord(1, "camera", "Nikon", "FM2", { mount: "F" }),
            gearRecord(2, "lens", "Mamiya", "80mm f/4", { mount: "Mamiya 7" }),
            gearRecord(3, "lens", "Nikon", "Nikkor 50mm f/1.8", { mount: "F" }),
            gearRecord(4, "chemistry", "Adox", "Rodinal", { process: "B&W" }),
        ],
        photoFolder: null,
    };
    (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: any) => {
            if (cmd.startsWith("catalog_") || cmd === "open_image_path") calls.push({ cmd, args: JSON.parse(JSON.stringify(args ?? {})) });
            if (cmd === "catalog_load") return structuredClone(catalog);
            if (cmd === "catalog_edit") return args.id === 2 ? STORED : null;
            if (cmd === "catalog_save_edit") return 1234;
            if (cmd === "catalog_create_gear") return { id: nextGear++, ...args.gear };
            if (cmd === "catalog_create_stock") return { id: 100 + calls.length, ...args.stock };
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
        const format = [...document.querySelectorAll<HTMLLabelElement>(".details .field")].find(
            (f) => f.querySelector(".label")?.textContent === "Format",
        )!;
        const input = format.querySelector("input")!;
        input.focus();
        await userEvent.keyboard("120");
        await vi.waitFor(() => {
            const last = called("catalog_update_roll").at(-1);
            expect(last?.args).toEqual({
                id: 10,
                name: "Portra in Lisbon",
                info: { filmStock: "Kodak Portra 400", format: "120" },
                stockId: 1,
                gear: { camera: null, lenses: [], scanner: null, chemistry: null },
            });
        });
        input.blur();
    });

    it("puts the roll on a stock picked from the list, with its box speed", async () => {
        const picker = $<HTMLInputElement>('[role=combobox][aria-label="Film stock"]')!;
        expect(picker.value).toBe("Kodak Portra 400");
        picker.focus();
        await userEvent.keyboard("tri");
        const option = await vi.waitFor(() => {
            const found = [...document.querySelectorAll<HTMLButtonElement>(".picker [role=option]")];
            expect(found.map((o) => o.textContent?.replace(/\s+/g, " ").trim())).toEqual([
                "Kodak Tri-X 400 B&W",
                "+ Add “tri” as a new stock",
                "No stock",
            ]);
            return found[0];
        });
        option.click();
        await vi.waitFor(() => {
            const last = called("catalog_update_roll").at(-1);
            expect(last?.args.stockId).toBe(2);
            expect(last?.args.info).toMatchObject({ filmStock: "Kodak Tri-X 400", boxIso: "400", ratedIso: "400" });
        });
        expect($(".roll-row .name")).not.toBeNull();
    });

    it("asks a lab-developed roll for the lab and its chemistry instead of a recipe", async () => {
        const labels = () => [...document.querySelectorAll(".details .field .label")].map((l) => l.textContent);
        expect(labels()).toContain("Dilution");
        [...document.querySelectorAll<HTMLButtonElement>(".details [role=radio]")].find((b) => b.textContent === "Lab")!.click();
        await vi.waitFor(() => expect(labels()).toContain("Chemistry"));
        expect(labels()).toContain("Lab");
        expect(labels()).toContain("Push / pull");
        expect(labels()).not.toContain("Dilution");
        expect(labels()).not.toContain("Developer");
    });

    it("takes the camera, lenses and chemistry from the catalog's gear", async () => {
        const pick = async (label: string, typed: string, option: string) => {
            const field = $<HTMLInputElement>(`[role=combobox][aria-label="${label}"]`)!;
            field.focus();
            await userEvent.keyboard(typed);
            const button = await vi.waitFor(() => {
                const found = [...document.querySelectorAll<HTMLButtonElement>(".picker [role=option]")].find((o) =>
                    o.textContent?.replace(/\s+/g, " ").includes(option),
                );
                expect(found).toBeDefined();
                return found!;
            });
            button.click();
        };
        [...document.querySelectorAll<HTMLButtonElement>(".details [role=radio]")].find((b) => b.textContent === "Home")!.click();
        await pick("Camera", "fm", "Nikon FM2");
        await vi.waitFor(() => expect(called("catalog_update_roll").at(-1)?.args.gear.camera).toBe(1));

        const lens = $<HTMLInputElement>('[role=combobox][aria-label="Add a lens"]')!;
        lens.focus();
        const offered = await vi.waitFor(() => {
            const names = [...document.querySelectorAll(".picker [role=option] .label")].map((o) => o.textContent);
            expect(names.length).toBeGreaterThan(0);
            return names;
        });
        expect(offered).toEqual(["Nikon Nikkor 50mm f/1.8", "Set up a new lens in Settings…"]);
        lens.blur();
        await pick("Add a lens", "nikkor", "Nikon Nikkor 50mm f/1.8");
        await pick("Add a lens", "Helios 44-2", "Set up “Helios 44-2” in Settings…");
        await vi.waitFor(() => expect(settings.open).toBe(true));
        settings.close();
        await vi.waitFor(() => expect(called("catalog_update_roll").at(-1)?.args.gear.lenses).toEqual([3, 10]));
        expect(called("catalog_create_gear").at(-1)?.args.gear).toMatchObject({ kind: "lens", name: "Helios 44-2", mount: "F" });
        expect([...document.querySelectorAll(".lenses .chip .name")].map((c) => c.textContent)).toEqual([
            "Nikon Nikkor 50mm f/1.8",
            "Helios 44-2",
        ]);

        await pick("Developer", "rod", "Adox Rodinal");
        await vi.waitFor(() => {
            const last = called("catalog_update_roll").at(-1)?.args;
            expect(last.gear.chemistry).toBe(4);
            expect(last.info).toMatchObject({ developer: "Adox Rodinal" });
        });
    });

    it("sets up new gear in Settings from the roll, already on the roll", async () => {
        const SettingsModal = (await import("$lib/components/settings/SettingsModal.svelte")).default;
        const modal = mount(SettingsModal, { target: document.body });
        const developer = $<HTMLInputElement>('[role=combobox][aria-label="Developer"]')!;
        developer.focus();
        await userEvent.keyboard("HC-110");
        const labels = await vi.waitFor(() => {
            const found = [...document.querySelectorAll(".picker [role=option]")].map((o) => o.textContent?.replace(/\s+/g, " ").trim());
            expect(found.length).toBeGreaterThan(0);
            return found;
        });
        expect(labels).toEqual(["Set up “HC-110” in Settings…", "No chemistry setup"]);
        [...document.querySelectorAll<HTMLButtonElement>(".picker [role=option]")][0].click();
        await vi.waitFor(() => expect(settings.open).toBe(true));
        expect(settings.section).toBe("home-lab");
        expect(called("catalog_create_gear").at(-1)?.args.gear).toMatchObject({ kind: "chemistry", name: "HC-110" });
        const row = await vi.waitFor(() => {
            const open = $(".record.open");
            expect(open).not.toBeNull();
            return open!;
        });
        expect(row.querySelector<HTMLInputElement>("input[placeholder='Rodinal']")?.value).toBe("HC-110");
        await vi.waitFor(() => expect(called("catalog_update_roll").at(-1)?.args.gear.chemistry).toBe(nextGear - 1));
        settings.close();
        await vi.waitFor(() => expect($(".record")).toBeNull());
        unmount(modal);
    });

    it("adds a scanner by its name alone", async () => {
        const scanner = $<HTMLInputElement>('[role=combobox][aria-label="Scanner"]')!;
        scanner.focus();
        await userEvent.keyboard("Nikon Coolscan V");
        const labels = await vi.waitFor(() => {
            const found = [...document.querySelectorAll(".picker [role=option]")].map((o) => o.textContent?.replace(/\s+/g, " ").trim());
            expect(found.length).toBeGreaterThan(0);
            return found;
        });
        expect(labels).toEqual(["+ Add “Nikon Coolscan V” as a new scanner"]);
        [...document.querySelectorAll<HTMLButtonElement>(".picker [role=option]")][0].click();
        await vi.waitFor(() => expect(called("catalog_update_roll").at(-1)?.args.gear.scanner).toBe(nextGear - 1));
        expect(called("catalog_create_gear").at(-1)?.args.gear).toMatchObject({ kind: "scanner", name: "Nikon Coolscan V" });
    });

    it("keeps the dates a roll was shot over as a range", async () => {
        $<HTMLInputElement>('input[aria-label="Shot from"]')!.focus();
        await userEvent.keyboard("March 2024");
        $<HTMLInputElement>('input[aria-label="Shot until"]')!.focus();
        await userEvent.keyboard("May 2024");
        await vi.waitFor(() =>
            expect(called("catalog_update_roll").at(-1)?.args.info).toMatchObject({ dateShot: "March 2024", dateShotEnd: "May 2024" }),
        );
        (document.activeElement as HTMLElement).blur();
    });

    it("renames the roll in place when its name is double-clicked", async () => {
        $(".head h1 .name-text")!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
        const field = await vi.waitFor(() => {
            const input = $<HTMLInputElement>('.head h1 input[aria-label="Roll name"]');
            expect(input).not.toBeNull();
            return input!;
        });
        await userEvent.fill(field, "Lisbon, spring");
        await userEvent.keyboard("{Enter}");
        await vi.waitFor(() => expect(called("catalog_update_roll").at(-1)?.args.name).toBe("Lisbon, spring"));
        expect($("h1")!.textContent).toBe("Lisbon, spring");
        expect($(".details .roll-name")!.textContent?.trim()).toBe("Lisbon, spring");
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

describe("film makers in Settings", () => {
    it("adds a maker as a group to add stocks to, and renames one across its stocks", async () => {
        const SettingsModal = (await import("$lib/components/settings/SettingsModal.svelte")).default;
        const modal = mount(SettingsModal, { target: document.body });
        settings.show("film-stocks");
        const button = (name: string, within: ParentNode = document) =>
            [...within.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === name);
        await vi.waitFor(() => expect(button("Add a film maker")).toBeDefined());
        button("Add a film maker")!.click();
        const field = await vi.waitFor(() => {
            const input = $<HTMLInputElement>('input[aria-label="Film maker"]');
            expect(input).not.toBeNull();
            return input!;
        });
        await userEvent.fill(field, "Ilford");
        await userEvent.keyboard("{Enter}");
        const group = await vi.waitFor(() => {
            const found = $('[data-maker="ilford"]');
            expect(found?.querySelector("h3")?.textContent?.trim()).toBe("Ilford");
            return found!;
        });
        button("Add a stock", group)!.click();
        await vi.waitFor(() => expect(called("catalog_create_stock").at(-1)?.args.stock.maker).toBe("Ilford"));

        $('[data-maker="kodak"] h3 .name-text')!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
        const title = await vi.waitFor(() => {
            const input = $<HTMLInputElement>('[data-maker="kodak"] h3 input');
            expect(input).not.toBeNull();
            return input!;
        });
        await userEvent.fill(title, "Eastman Kodak");
        await userEvent.keyboard("{Enter}");
        await vi.waitFor(() => expect($('[data-maker="eastman kodak"]')).not.toBeNull());
        expect(library.filmStocks.filter((s) => s.maker === "Eastman Kodak").map((s) => s.name)).toEqual(["Portra 400", "Tri-X 400"]);
        settings.close();
        unmount(modal);
    });
});
