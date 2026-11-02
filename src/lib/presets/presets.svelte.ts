// $lib/presets/presets.svelte.ts

/**
 * The saved presets and the panel that lists them. Each preset is stored as
 * the same document an export writes, so the list file, an exported file and
 * a clipboard copy are all read by one parser.
 */

import { invoke } from "@tauri-apps/api/core";
import { load, type Store } from "@tauri-apps/plugin-store";
import {
    type Preset,
    type PresetContent,
    parsePreset,
    serializePreset,
} from "./presetDocument";

export const PRESETS_FILE = "presets.json";
const PRESETS_KEY = "presets";

let opening: Promise<Store> | null = null;

function store() {
    opening ??= load(PRESETS_FILE);
    return opening;
}

let nextId = 0;

function withId(content: PresetContent): Preset {
    return { ...content, id: `preset-${++nextId}` };
}

class Presets {
    /** Whether the panel is open. */
    open = $state(false);

    list = $state<Preset[]>([]);

    readonly hydrated: Promise<void> = this.#load();

    async #load() {
        let documents: unknown;
        try {
            documents = await (await store()).get(PRESETS_KEY);
        } catch (error) {
            console.warn(`Presets could not reach ${PRESETS_FILE}:`, error);
            opening = null;
            return;
        }
        if (!Array.isArray(documents)) return;

        const loaded: Preset[] = [];
        for (const document of documents) {
            if (typeof document !== "string") continue;
            try {
                loaded.push(withId(parsePreset(document)));
            } catch (error) {
                console.warn("Skipped an unreadable preset:", error);
            }
        }
        // Anything added before the file was read stays, after what it held.
        this.list = [...loaded, ...this.list];
    }

    async #save() {
        const documents = this.list.map((preset) => serializePreset(preset));
        try {
            const file = await store();
            await file.set(PRESETS_KEY, documents);
            await file.save();
        } catch (error) {
            console.warn(`Presets could not be written to ${PRESETS_FILE}:`, error);
            opening = null;
        }
    }

    toggle() {
        this.open = !this.open;
    }

    add(content: PresetContent): Preset {
        const preset = withId(content);
        this.list = [...this.list, preset];
        void this.#save();
        return preset;
    }

    update(id: string, content: PresetContent) {
        this.list = this.list.map((preset) => (preset.id === id ? { ...content, id } : preset));
        void this.#save();
    }

    remove(id: string) {
        this.list = this.list.filter((preset) => preset.id !== id);
        void this.#save();
    }

    /** Resolves false when the save dialog is dismissed. */
    async exportFile(preset: Preset): Promise<boolean> {
        // Characters most file systems refuse are left out of the suggested name.
        const fileName = preset.name.replace(/[\\/:*?"<>|]/g, "").trim() || "preset";
        return invoke<boolean>("export_preset", { fileName, contents: serializePreset(preset) });
    }

    /** Adds every readable file the user picks, and returns how many were not. */
    async importFiles(): Promise<number> {
        const files = await invoke<{ name: string; contents: string }[]>("import_presets");
        let failed = 0;
        for (const file of files) {
            try {
                this.add(parsePreset(file.contents, file.name));
            } catch (error) {
                console.warn(`Could not import ${file.name}:`, error);
                failed++;
            }
        }
        return failed;
    }
}

export const presets = new Presets();
