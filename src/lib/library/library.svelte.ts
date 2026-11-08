// $lib/library/library.svelte.ts
//
// The catalog as the app sees it: libraries, rolls and images, what is being
// looked at and selected, their thumbnails, and the move between the library
// and the editor. See docs/library.md.

import { tick, untrack } from "svelte";

import { editor } from "$lib/editor/editor.svelte";
import { retouchTool } from "$lib/retouch/retouchTool.svelte";
import { presets } from "$lib/presets/presets.svelte";
import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
import { settings } from "$lib/settings/settings.svelte";
import { type ImageEdit, cloneImageEdit, newImageEdit, parseImageEdit, serializeImageEdit } from "$lib/types/imageEdit";
import type { FrameInfo, RollInfo } from "$lib/types/rollInfo";
import { type FilmStock, type StockFields, stockLabel } from "$lib/types/filmStock";
import { type Gear, type GearFields, type GearKind, type GearSlot, type RollGear, GEAR_SLOTS, NO_ROLL_GEAR, gearLabel } from "$lib/types/gear";
import { type EditHistory, parseEditHistory, serializeEditHistory } from "$lib/types/editHistory";
import { history } from "$lib/history/history.svelte";
import * as api from "./catalogApi";
import type { ImageRecord, LibraryRecord, Place, RollRecord } from "./catalogApi";
import { fitRect, fly, imageRect, land, nextFrame, rectOf, settle } from "./flight";

export type LibraryView = { kind: "library"; id: number } | { kind: "roll"; id: number };
export type MoveTarget = { label: string; place: Place; depth: number };

const SAVE_DELAY_MS = 400;
// Long enough that a slider drag renders once, when it settles.
const THUMBNAIL_DELAY_MS = 1200;
// How long leaving the editor waits for a fresh thumbnail before flying the last one.
const FRESH_THUMBNAIL_WAIT_MS = 350;
// The least time between two looks for moved files on coming back to the window.
const FOCUS_SEARCH_GAP_MS = 30_000;

/** The open image's edit and undo history, as they are stored. */
type Saved = { edit: string; history: string };

function openSaved(): Saved {
    return {
        edit: serializeImageEdit(editor.edit),
        history: serializeEditHistory({ steps: history.stack, pointer: history.pointer }),
    };
}

const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** `base`, or `base 2`, `base 3`… whichever is not taken yet. */
function freeName(base: string, taken: string[]): string {
    if (!taken.includes(base)) return base;
    let n = 2;
    while (taken.includes(`${base} ${n}`)) n++;
    return `${base} ${n}`;
}

/** Writes that wait for changes to settle, and can all be made at once. */
class SaveQueue {
    #waiting = new Map<string, { timer: ReturnType<typeof setTimeout>; write: () => Promise<void> }>();

    schedule(key: string, write: () => Promise<void>) {
        clearTimeout(this.#waiting.get(key)?.timer);
        this.#waiting.set(key, { timer: setTimeout(() => void this.#write(key), SAVE_DELAY_MS), write });
    }

    #write(key: string): Promise<void> {
        const waiting = this.#waiting.get(key);
        if (!waiting) return Promise.resolve();
        this.#waiting.delete(key);
        clearTimeout(waiting.timer);
        return waiting.write();
    }

    flush() {
        return Promise.all([...this.#waiting.keys()].map((key) => this.#write(key)));
    }
}

class Library {
    libraries = $state.raw<LibraryRecord[]>([]);
    rolls = $state.raw<RollRecord[]>([]);
    images = $state.raw<ImageRecord[]>([]);
    filmStocks = $state.raw<FilmStock[]>([]);
    gear = $state.raw<Gear[]>([]);
    /** Gear just made from a roll, for Settings to open on. */
    editingGearId = $state<number | null>(null);
    photoFolder = $state<string | null>(null);
    loaded = $state(false);

    view = $state<LibraryView | null>(null);
    /** Selected image ids, in the order they were picked. */
    selection = $state.raw<number[]>([]);
    #anchor: number | null = null;

    /** Object URLs of each image's thumbnail. */
    thumbs = $state.raw<Record<number, string>>({});
    #thumbsAsked = new Set<number>();
    #renderQueue = new Map<number, ImageEdit | null>();
    #rendering = false;

    /** What fills the window. */
    showing = $state<"library" | "editor">("library");
    /** The image whose cell is hidden while it is in flight. */
    flyingId = $state<number | null>(null);
    /** The editor's canvas is held back until the image is on it. */
    canvasHidden = $state(false);
    /** On the way back to the library: the image stays on the canvas until it takes off. */
    leaving = $state(false);
    #moving = false;

    /** A library being named in place. */
    renamingId = $state<number | null>(null);
    renamingRollId = $state<number | null>(null);
    /** A message for the user, with an action to confirm when there is one. */
    notice = $state<{ text: string; confirm?: { label: string; run: () => void } } | null>(null);

    // ===== What is being looked at =====

    currentRoll = $derived(
        this.view?.kind === "roll" ? (this.rolls.find((r) => r.id === this.view!.id) ?? null) : null,
    );
    currentLibrary = $derived.by(() => {
        const id = this.view?.kind === "library" ? this.view.id : this.currentRoll?.libraryId;
        return this.libraries.find((l) => l.id === id) ?? null;
    });
    /**
     * The images the grid shows: a roll's frames, or a library's loose images.
     * With rolls not grouped, a library shows every image in it, each roll's
     * frames in turn and then the loose ones.
     */
    shownImages = $derived.by(() => {
        const view = this.view;
        if (!view) return [];
        if (view.kind === "roll") return this.framesOf(view.id);
        const loose = this.images.filter((i) => i.libraryId === view.id && i.rollId === null);
        if (settings.groupRolls) return loose;
        return [...this.rollsOf(view.id).flatMap((r) => this.framesOf(r.id)), ...loose];
    });
    shownRolls = $derived(this.view?.kind === "library" && settings.groupRolls ? this.rollsOf(this.view.id) : []);
    selectedImages = $derived(
        this.selection.map((id) => this.images.find((i) => i.id === id)).filter((i): i is ImageRecord => !!i),
    );
    /** Where an import goes: the roll or library being looked at. */
    place = $derived<Place | null>(
        this.currentRoll
            ? { libraryId: this.currentRoll.libraryId, rollId: this.currentRoll.id }
            : this.currentLibrary
              ? { libraryId: this.currentLibrary.id, rollId: null }
              : null,
    );

    rollsOf(libraryId: number) {
        return this.rolls.filter((r) => r.libraryId === libraryId);
    }

    framesOf(rollId: number) {
        return this.images.filter((i) => i.rollId === rollId).sort((a, b) => (a.frame ?? 0) - (b.frame ?? 0));
    }

    countIn(libraryId: number) {
        return this.images.filter((i) => i.libraryId === libraryId).length;
    }

    /** Every library and roll an image can go to, in the order the side list shows them. */
    moveTargets = $derived(
        this.libraries.flatMap((l): MoveTarget[] => [
            { label: l.name, place: { libraryId: l.id, rollId: null }, depth: 0 },
            ...this.rollsOf(l.id).map((r) => ({ label: r.name, place: { libraryId: l.id, rollId: r.id }, depth: 1 })),
        ]),
    );

    // ===== Loading =====

    load = async () => {
        try {
            const snapshot = await api.loadCatalog();
            this.libraries = snapshot.libraries;
            this.rolls = snapshot.rolls.map((r) => ({ ...r, gear: { ...NO_ROLL_GEAR, ...r.gear } }));
            this.images = snapshot.images;
            this.filmStocks = snapshot.filmStocks;
            this.gear = snapshot.gear ?? [];
            this.photoFolder = snapshot.photoFolder;
            if (!this.view && this.libraries.length > 0) this.view = { kind: "library", id: this.libraries[0].id };
            if (this.images.some((i) => i.missing)) void this.findMoved();
        } catch (e) {
            this.#fail("The catalog could not be read", e);
        } finally {
            this.loaded = true;
        }
    };

    #fail(text: string, e?: unknown) {
        if (e !== undefined) console.error(`${text}:`, e);
        const detail = typeof e === "string" ? e : e instanceof Error ? e.message : "";
        this.notice = { text: detail ? `${text}: ${detail}` : text };
    }

    show = (view: LibraryView) => {
        this.view = view;
        this.selection = [];
        this.#anchor = null;
    };

    // ===== Libraries and rolls =====

    createLibrary = async (name?: string) => {
        try {
            const library = await api.createLibrary(name ?? freeName("New library", this.libraries.map((l) => l.name)));
            this.libraries = [...this.libraries, library];
            this.show({ kind: "library", id: library.id });
            if (!name) this.renamingId = library.id;
            return library;
        } catch (e) {
            this.#fail("The library could not be made", e);
            return null;
        }
    };

    renameLibrary = async (id: number, name: string) => {
        this.renamingId = null;
        const trimmed = name.trim();
        if (!trimmed) return;
        this.libraries = this.libraries.map((l) => (l.id === id ? { ...l, name: trimmed } : l));
        try {
            await api.renameLibrary(id, trimmed);
        } catch (e) {
            this.#fail("The library could not be renamed", e);
        }
    };

    deleteLibrary = (id: number) => {
        const library = this.libraries.find((l) => l.id === id);
        if (!library) return;
        const count = this.countIn(id);
        this.notice = {
            text: `Delete “${library.name}”${count ? ` and its ${count} image record${count === 1 ? "" : "s"}` : ""}? The files stay on disk.`,
            confirm: {
                label: "Delete",
                run: async () => {
                    try {
                        await api.deleteLibrary(id);
                        this.#forget(this.images.filter((i) => i.libraryId === id).map((i) => i.id));
                        this.libraries = this.libraries.filter((l) => l.id !== id);
                        this.rolls = this.rolls.filter((r) => r.libraryId !== id);
                        if (this.currentLibrary === null || this.currentLibrary.id === id) {
                            this.view = this.libraries[0] ? { kind: "library", id: this.libraries[0].id } : null;
                        }
                    } catch (e) {
                        this.#fail("The library could not be deleted", e);
                    }
                },
            },
        };
    };

    createRoll = async (libraryId: number) => {
        const taken = this.rollsOf(libraryId).map((r) => r.name);
        try {
            const roll = await api.createRoll(libraryId, freeName(`Roll ${taken.length + 1}`, taken));
            this.rolls = [...this.rolls, roll];
            this.show({ kind: "roll", id: roll.id });
            return roll;
        } catch (e) {
            this.#fail("The roll could not be made", e);
            return null;
        }
    };

    #saves = new SaveQueue();

    updateRoll = (
        id: number,
        patch: { name?: string; info?: RollInfo; stockId?: number | null; gear?: Partial<RollGear> },
    ) => {
        this.rolls = this.rolls.map((r) =>
            r.id === id
                ? {
                      ...r,
                      name: patch.name ?? r.name,
                      info: { ...r.info, ...patch.info },
                      stockId: patch.stockId === undefined ? r.stockId : patch.stockId,
                      gear: { ...r.gear, ...patch.gear },
                  }
                : r,
        );
        this.#saves.schedule(`roll ${id}`, async () => {
            const roll = this.rolls.find((r) => r.id === id);
            if (!roll) return;
            const name = roll.name.trim() || "Untitled roll";
            await api.updateRoll(id, name, roll.info, roll.stockId, roll.gear).catch((e) => this.#fail("The roll could not be saved", e));
        });
    };

    // ===== Film stocks =====

    stockOf(roll: RollRecord | null): FilmStock | null {
        return roll?.stockId == null ? null : (this.filmStocks.find((s) => s.id === roll.stockId) ?? null);
    }

    /** What a roll was shot on: its stock's name, or what was typed for it. */
    stockNameOf(roll: RollRecord): string {
        const stock = this.stockOf(roll);
        return stock ? stockLabel(stock) : (roll.info.filmStock ?? "");
    }

    /**
     * Puts a roll on a stock, or takes it off one. The box speed comes from the
     * stock, and the rated speed follows it unless it was set apart.
     */
    setRollStock = (rollId: number, stock: FilmStock | null) => {
        const roll = this.rolls.find((r) => r.id === rollId);
        if (!roll) return;
        if (!stock) {
            this.updateRoll(rollId, { stockId: null, info: { filmStock: "" } });
            return;
        }
        const info: RollInfo = { filmStock: stockLabel(stock) };
        if (stock.boxIso !== null) {
            const iso = String(stock.boxIso);
            if (!roll.info.ratedIso || roll.info.ratedIso === roll.info.boxIso) info.ratedIso = iso;
            info.boxIso = iso;
        }
        if (!roll.info.format && stock.formats.length === 1) info.format = stock.formats[0];
        this.updateRoll(rollId, { stockId: stock.id, info });
    };

    createStock = async (fields: StockFields): Promise<FilmStock | null> => {
        try {
            const stock = await api.createStock(fields);
            this.filmStocks = [...this.filmStocks, stock];
            return stock;
        } catch (e) {
            this.#fail("The film stock could not be added", e);
            return null;
        }
    };

    updateStock = (id: number, patch: Partial<StockFields>) => {
        this.filmStocks = this.filmStocks.map((s) => (s.id === id ? { ...s, ...patch } : s));
        this.#saves.schedule(`stock ${id}`, async () => {
            const stock = this.filmStocks.find((s) => s.id === id);
            if (!stock) return;
            const { id: _, ...fields } = stock;
            await api.updateStock(id, fields).catch((e) => this.#fail("The film stock could not be saved", e));
        });
    };

    deleteStock = async (id: number) => {
        const stock = this.filmStocks.find((s) => s.id === id);
        if (!stock) return;
        try {
            await api.deleteStock(id);
            const label = stockLabel(stock);
            this.filmStocks = this.filmStocks.filter((s) => s.id !== id);
            this.rolls = this.rolls.map((r) =>
                r.stockId === id ? { ...r, stockId: null, info: { ...r.info, filmStock: label } } : r,
            );
        } catch (e) {
            this.#fail("The film stock could not be deleted", e);
        }
    };

    // ===== Gear =====

    gearOf(id: number | null): Gear | null {
        return id === null ? null : (this.gear.find((g) => g.id === id) ?? null);
    }

    gearOfKind(kind: GearKind): Gear[] {
        return this.gear.filter((g) => g.kind === kind);
    }

    /** Puts a roll on a camera, scanner, chemistry or lab, or takes it off one. */
    setRollGear = (rollId: number, slot: GearSlot, gear: Gear | null) => {
        const info: RollInfo = { [GEAR_SLOTS[slot]]: gear ? gearLabel(gear) : "" };
        this.updateRoll(rollId, { gear: { [slot]: gear?.id ?? null }, info });
    };

    addRollLens = (rollId: number, lens: Gear) => {
        const roll = this.rolls.find((r) => r.id === rollId);
        if (!roll || roll.gear.lenses.includes(lens.id)) return;
        this.updateRoll(rollId, { gear: { lenses: [...roll.gear.lenses, lens.id] } });
    };

    removeRollLens = (rollId: number, lensId: number) => {
        const roll = this.rolls.find((r) => r.id === rollId);
        if (roll) this.updateRoll(rollId, { gear: { lenses: roll.gear.lenses.filter((l) => l !== lensId) } });
    };

    createGear = async (fields: GearFields): Promise<Gear | null> => {
        try {
            const gear = await api.createGear(fields);
            this.gear = [...this.gear, gear];
            return gear;
        } catch (e) {
            this.#fail("The gear could not be added", e);
            return null;
        }
    };

    updateGear = (id: number, patch: Partial<GearFields>) => {
        this.gear = this.gear.map((g) => (g.id === id ? { ...g, ...patch } : g));
        this.#saves.schedule(`gear ${id}`, async () => {
            const gear = this.gear.find((g) => g.id === id);
            if (!gear) return;
            const { id: _, ...fields } = gear;
            await api.updateGear(id, fields).catch((e) => this.#fail("The gear could not be saved", e));
        });
    };

    /** Removes a piece of gear. The rolls that used it keep its name, as the catalog does. */
    deleteGear = async (id: number) => {
        const gear = this.gear.find((g) => g.id === id);
        if (!gear) return;
        try {
            await api.deleteGear(id);
            const label = gearLabel(gear);
            this.gear = this.gear.filter((g) => g.id !== id);
            this.rolls = this.rolls.map((r) => {
                const links = { ...r.gear, lenses: r.gear.lenses.filter((l) => l !== id) };
                const info = { ...r.info };
                if (links.lenses.length !== r.gear.lenses.length) {
                    info.lenses = info.lenses?.trim() ? `${info.lenses.trim()}, ${label}` : label;
                }
                for (const [slot, key] of Object.entries(GEAR_SLOTS) as [GearSlot, (typeof GEAR_SLOTS)[GearSlot]][]) {
                    if (links[slot] === id) {
                        links[slot] = null;
                        info[key] = label;
                    }
                }
                return { ...r, gear: links, info };
            });
        } catch (e) {
            this.#fail("The gear could not be deleted", e);
        }
    };

    deleteRoll = (id: number) => {
        const roll = this.rolls.find((r) => r.id === id);
        if (!roll) return;
        this.notice = {
            text: `Delete the roll “${roll.name}”? Its frames stay in the library, loose.`,
            confirm: {
                label: "Delete roll",
                run: async () => {
                    try {
                        await api.deleteRoll(id);
                        this.rolls = this.rolls.filter((r) => r.id !== id);
                        this.images = this.images.map((i) => (i.rollId === id ? { ...i, rollId: null, frame: null } : i));
                        if (this.view?.kind === "roll" && this.view.id === id) {
                            this.show({ kind: "library", id: roll.libraryId });
                        }
                    } catch (e) {
                        this.#fail("The roll could not be deleted", e);
                    }
                },
            },
        };
    };

    // ===== Importing =====

    /** The place being looked at, making a first library if there is none. */
    async #placeForImport(): Promise<Place | null> {
        if (this.place) return this.place;
        const library = this.libraries[0] ?? (await this.createLibrary("My library"));
        if (!library) return null;
        this.show({ kind: "library", id: library.id });
        return { libraryId: library.id, rollId: null };
    }

    #added(imported: api.Imported) {
        if (imported.roll) this.rolls = [...this.rolls, imported.roll];
        const known = new Set(this.images.map((i) => i.id));
        this.images = [...this.images, ...imported.images.filter((i) => !known.has(i.id))];
        if (imported.skipped.length > 0) this.#reportSkipped(imported.skipped);
    }

    #reportSkipped(skipped: api.Imported["skipped"]) {
        const names = skipped.map((s) => s.path.split(/[\\/]/).pop() ?? s.path);
        const listed = names.length > 3 ? `${names.slice(0, 3).join(", ")} and ${names.length - 3} more` : names.join(", ");
        const reasons = new Set(skipped.map((s) => s.reason));
        const reason = reasons.size === 1 ? `: ${skipped[0].reason}` : "";
        console.error("Files not imported:", skipped);
        this.notice = { text: `Could not import ${listed}${reason}` };
    }

    importFiles = async () => {
        const place = await this.#placeForImport();
        if (!place) return;
        try {
            const imported = await api.importFiles(place, settings.importCopies);
            this.#added(imported);
            this.selection = imported.images.map((i) => i.id);
        } catch (e) {
            this.#fail("The images could not be imported", e);
        }
    };

    importFolder = async () => {
        const place = await this.#placeForImport();
        if (!place) return;
        try {
            const imported = await api.importFolder(place.libraryId, settings.importCopies);
            this.#added(imported);
            if (imported.roll) this.show({ kind: "roll", id: imported.roll.id });
        } catch (e) {
            this.#fail("The folder could not be imported", e);
        }
    };

    /** File > Open: picks a file, adds it where the library is looking, and opens it. */
    openFile = async () => {
        const place = await this.#placeForImport();
        if (!place) return;
        try {
            const record = await api.openFile(place, settings.importCopies);
            if (!record) return;
            this.#added({ roll: null, images: [record], skipped: [] });
            this.show(record.rollId ? { kind: "roll", id: record.rollId } : { kind: "library", id: record.libraryId });
            this.selection = [record.id];
            await this.openImage(record.id);
        } catch (e) {
            this.#fail("The image could not be opened", e);
        }
    };

    // ===== Selection =====

    select = (id: number, { toggle = false, range = false } = {}) => {
        const order = this.shownImages.map((i) => i.id);
        if (range && this.#anchor !== null && order.includes(this.#anchor)) {
            const [a, b] = [order.indexOf(this.#anchor), order.indexOf(id)].sort((x, y) => x - y);
            const span = order.slice(a, b + 1);
            this.selection = toggle ? [...new Set([...this.selection, ...span])] : span;
            return;
        }
        if (toggle) {
            this.selection = this.selection.includes(id)
                ? this.selection.filter((s) => s !== id)
                : [...this.selection, id];
        } else {
            this.selection = [id];
        }
        this.#anchor = id;
    };

    selectAll = () => {
        this.selection = this.shownImages.map((i) => i.id);
    };

    // ===== Moving and removing =====

    moveImages = async (ids: number[], place: Place) => {
        if (ids.length === 0) return;
        try {
            await api.moveImages(ids, place);
            await this.load();
            this.selection = this.selection.filter((id) => !ids.includes(id) || this.shownImages.some((i) => i.id === id));
        } catch (e) {
            this.#fail("The images could not be moved", e);
        }
    };

    removeImages = (ids: number[]) => {
        if (ids.length === 0) return;
        const n = ids.length;
        this.notice = {
            text: `Remove ${n === 1 ? "this image" : `${n} images`} from the catalog, with ${n === 1 ? "its edit" : "their edits"}? The files stay on disk.`,
            confirm: {
                label: "Remove",
                run: async () => {
                    try {
                        await api.removeImages(ids);
                        this.#forget(ids);
                        this.images = this.images.filter((i) => !ids.includes(i.id));
                        this.selection = this.selection.filter((id) => !ids.includes(id));
                    } catch (e) {
                        this.#fail("The images could not be removed", e);
                    }
                },
            },
        };
    };

    locate = async (id: number) => {
        try {
            const record = await api.locateImage(id);
            if (!record) return;
            this.#putBack([record]);
            await this.findMoved(record.path);
        } catch (e) {
            this.#fail("The file could not be used", e);
        }
    };

    /** Whether missing files are being looked for. */
    finding = $state(false);

    /** Looks for the files that are not where they were, and points their images at them. */
    findMoved = async (near: string | null = null) => {
        if (this.finding || this.images.length === 0) return;
        this.finding = true;
        try {
            const { found, missing } = await api.findMoved(near);
            const still = new Set(missing);
            this.images = this.images.map((i) => (i.missing !== still.has(i.id) ? { ...i, missing: still.has(i.id) } : i));
            this.#putBack(found);
            if (found.length > 0 && !this.notice?.confirm) {
                const files = found.length === 1 ? "file" : "files";
                this.notice = { text: `Found ${found.length} ${near ? "more " : ""}${files} that had moved.` };
            }
        } catch (e) {
            console.error("Failed to look for moved files:", e);
        } finally {
            this.finding = false;
        }
    };

    #lastFocusSearch = -Infinity;

    /** On coming back to the window, where files may have been moved meanwhile. */
    findMovedOnFocus = () => {
        if (performance.now() - this.#lastFocusSearch < FOCUS_SEARCH_GAP_MS) return;
        this.#lastFocusSearch = performance.now();
        void this.findMoved();
    };

    #putBack(records: ImageRecord[]) {
        const byId = new Map(records.map((r) => [r.id, r]));
        this.images = this.images.map((i) => byId.get(i.id) ?? i);
        for (const record of records) {
            this.#thumbsAsked.delete(record.id);
            void this.requestThumbnail(record);
        }
    }

    setFrameInfo = (id: number, patch: FrameInfo) => {
        this.images = this.images.map((i) => (i.id === id ? { ...i, frameInfo: { ...i.frameInfo, ...patch } } : i));
        this.#saves.schedule(`frame ${id}`, async () => {
            const image = this.images.find((i) => i.id === id);
            if (image) await api.setFrameInfo(id, image.frameInfo).catch((e) => this.#fail("The frame could not be saved", e));
        });
    };

    #forget(ids: number[]) {
        const thumbs = { ...this.thumbs };
        for (const id of ids) {
            if (thumbs[id]) URL.revokeObjectURL(thumbs[id]);
            delete thumbs[id];
            this.#thumbsAsked.delete(id);
            this.#renderQueue.delete(id);
        }
        this.thumbs = thumbs;
    }

    // ===== Thumbnails =====

    /** Shows the stored thumbnail, or renders one if there is none yet. */
    requestThumbnail = async (image: ImageRecord) => {
        if (this.#thumbsAsked.has(image.id)) return;
        this.#thumbsAsked.add(image.id);
        try {
            const bytes = await api.fetchThumbnail(image.id);
            if (bytes) this.#setThumb(image.id, bytes);
            else if (!image.missing) this.#queueRender(image.id, null);
        } catch (e) {
            console.error("Failed to read thumbnail:", e);
        }
    };

    #setThumb(id: number, bytes: ArrayBuffer) {
        if (!this.images.some((i) => i.id === id)) return;
        const old = this.thumbs[id];
        this.thumbs = { ...this.thumbs, [id]: URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })) };
        if (old) URL.revokeObjectURL(old);
    }

    /** Renders with `edit`, or with the stored edit when it is null. A newer request replaces an older one. */
    #queueRender(id: number, edit: ImageEdit | null) {
        this.#renderQueue.delete(id);
        this.#renderQueue.set(id, edit);
        void this.#pumpRenders();
    }

    #renderDone = new Map<number, (() => void)[]>();

    /** Resolves once a thumbnail queued for `id` has rendered, or failed to. */
    #rendered(id: number): Promise<void> {
        if (!this.#renderQueue.has(id) && !this.#rendering) return Promise.resolve();
        return new Promise((resolve) => this.#renderDone.set(id, [...(this.#renderDone.get(id) ?? []), resolve]));
    }

    async #pumpRenders() {
        if (this.#rendering) return;
        this.#rendering = true;
        try {
            // The most recently asked for goes first: it is the one being looked at.
            while (this.#renderQueue.size > 0) {
                const [id, given] = [...this.#renderQueue].at(-1)!;
                this.#renderQueue.delete(id);
                try {
                    const edit = given ?? (await this.#storedEdit(id)) ?? newImageEdit(1, 1);
                    this.#setThumb(id, await api.renderThumbnail(id, edit, colorSpaceDebug.toneMap));
                } catch (e) {
                    console.error(`Failed to render thumbnail ${id}:`, e);
                }
                for (const resolve of this.#renderDone.get(id) ?? []) resolve();
                this.#renderDone.delete(id);
            }
        } finally {
            this.#rendering = false;
        }
    }

    async #storedEdit(id: number): Promise<ImageEdit | null> {
        const text = await api.fetchEdit(id);
        return text ? parseImageEdit(text) : null;
    }

    // ===== Saving the open image's edit =====

    #lastSaved = new Map<number, Saved>();
    #pendingSave: ({ id: number } & Saved) | null = null;
    #saveTimer: ReturnType<typeof setTimeout> | null = null;
    #thumbTimer: ReturnType<typeof setTimeout> | null = null;

    #scheduleSave(id: number, saved: Saved) {
        const last = this.#lastSaved.get(id);
        if (last?.edit === saved.edit && last.history === saved.history) return;
        this.#pendingSave = { id, ...saved };
        if (this.#saveTimer) clearTimeout(this.#saveTimer);
        this.#saveTimer = setTimeout(() => void this.flushSave(), SAVE_DELAY_MS);
        if (last?.edit === saved.edit) return;
        if (this.#thumbTimer) clearTimeout(this.#thumbTimer);
        this.#thumbTimer = setTimeout(() => this.#rerenderOpen(), THUMBNAIL_DELAY_MS);
    }

    /** Writes the open image's edit now rather than when the edits settle. */
    flushSave = async () => {
        if (this.#saveTimer) clearTimeout(this.#saveTimer);
        this.#saveTimer = null;
        const pending = this.#pendingSave;
        this.#pendingSave = null;
        if (!pending) return;
        const { id, ...saved } = pending;
        try {
            const edited = await api.saveEdit(id, saved.edit, saved.history);
            this.#lastSaved.set(id, saved);
            this.images = this.images.map((i) => (i.id === id ? { ...i, edited } : i));
        } catch (e) {
            this.#fail("The edit could not be saved", e);
        }
    };

    /** Writes every change still waiting to be saved. */
    flushAll = async () => {
        await Promise.all([this.#saves.flush(), this.flushSave()]);
    };

    /** Renders the open image's thumbnail with its edit as it stands. */
    #rerenderOpen() {
        if (this.#thumbTimer) clearTimeout(this.#thumbTimer);
        this.#thumbTimer = null;
        const id = editor.libraryImageId;
        if (id === null || !editor.imagePayload) return;
        this.#queueRender(id, cloneImageEdit($state.snapshot(editor.edit) as ImageEdit));
    }

    // ===== Between the library and the editor =====

    #cell(id: number) {
        return document.querySelector<HTMLImageElement>(`[data-thumb-id="${id}"]`);
    }

    async #openInEditor(image: ImageRecord): Promise<boolean> {
        await this.flushSave();
        let stored: ImageEdit | null = null;
        let storedHistory: EditHistory | null = null;
        try {
            [stored, storedHistory] = await Promise.all([
                this.#storedEdit(image.id),
                api.fetchEditHistory(image.id).then(parseEditHistory),
            ]);
        } catch (e) {
            console.error("Failed to read the stored edit:", e);
        }
        const opened = await editor.openLibraryImage(image.path, image.id, stored, storedHistory);
        if (opened) this.#lastSaved.set(image.id, openSaved());
        return opened;
    }

    /** Opens an image, flying it from its cell into the editor. */
    openImage = async (id: number) => {
        const image = this.images.find((i) => i.id === id);
        if (!image || this.#moving) return;
        if (image.missing) {
            this.notice = {
                text: `${image.fileName} is not where it was.`,
                confirm: { label: "Locate…", run: () => void this.locate(id) },
            };
            return;
        }
        this.#moving = true;
        try {
            const same = editor.libraryImageId === id && !!editor.imagePayload;
            const src = this.thumbs[id];
            const from = imageRect(this.#cell(id));

            this.flyingId = id;
            this.canvasHidden = true;
            this.showing = "editor";
            await tick();

            const opening = same ? Promise.resolve(true) : this.#openInEditor(image);
            let flyer: HTMLImageElement | null = null;
            const box = rectOf(document.querySelector(".canvas-center"));
            if (src && from && box) flyer = await fly(src, from, fitRect(from.width / from.height, box));

            if (!(await opening)) {
                flyer?.remove();
                this.showing = "library";
                this.#fail(`${image.fileName} could not be opened`);
                void this.findMoved();
                return;
            }
            // Give the canvas its first frames before taking the flyer away.
            if (!same) for (let i = 0; i < 4; i++) await nextFrame();
            // The editor never fits an image past its own pixels, so a small one shows smaller than the slot.
            const shown = rectOf(document.querySelector(".preview-frame"));
            if (flyer && shown) await settle(flyer, shown);
            this.canvasHidden = false;
            if (flyer) await land(flyer);
        } finally {
            this.flyingId = null;
            this.canvasHidden = false;
            this.#moving = false;
        }
    };

    /** Back to the library, flying the open image into its cell. */
    showLibrary = async () => {
        if (this.showing === "library" || this.#moving) return;
        this.#moving = true;
        this.leaving = true;
        try {
            if (editor.isCropping) editor.applyCrop();
            retouchTool.mode = null;
            presets.open = false;
            const id = editor.libraryImageId;
            const image = id === null ? undefined : this.images.find((i) => i.id === id);
            if (!image || !editor.imagePayload) {
                this.showing = "library";
                return;
            }

            void this.flushSave();
            this.#rerenderOpen();
            const fresh = this.#rendered(image.id);

            editor.zoom = null;
            if (!this.shownImages.some((i) => i.id === image.id)) {
                this.show(image.rollId !== null ? { kind: "roll", id: image.rollId } : { kind: "library", id: image.libraryId });
            }
            this.selection = [image.id];
            this.flyingId = image.id;
            this.showing = "library";
            // The side bars slide off while the thumbnail catches up with the edit.
            await Promise.race([fresh, wait(FRESH_THUMBNAIL_WAIT_MS)]);
            await tick();
            await nextFrame();

            const from = rectOf(document.querySelector(".preview-frame"));
            const cell = this.#cell(image.id);
            cell?.scrollIntoView({ block: "nearest" });
            const src = this.thumbs[image.id];
            this.canvasHidden = true;
            if (src && from && cell) {
                const flyer = await fly(src, from, imageRect(cell) ?? from);
                this.flyingId = null;
                await tick();
                flyer.remove();
            }
        } finally {
            this.flyingId = null;
            this.canvasHidden = false;
            this.leaving = false;
            this.#moving = false;
        }
    };

    /** Between the two, for the shortcut: into the open image, or back out of it. */
    toggleView = () => {
        if (this.showing === "editor") void this.showLibrary();
        else if (editor.libraryImageId !== null) void this.openImage(editor.libraryImageId);
    };

    constructor() {
        $effect.root(() => {
            // The open image's edit is saved as it changes.
            $effect(() => {
                const id = editor.libraryImageId;
                if (id === null || !editor.imagePayload) return;
                const saved = openSaved();
                untrack(() => this.#scheduleSave(id, saved));
            });
        });
    }
}

export const library = new Library();
