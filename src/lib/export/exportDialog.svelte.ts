// $lib/export/exportDialog.svelte.ts
//
// The export modal's state: what it will write, where, and how far along a
// running export is. The settings and the folder outlive the session.

import { listen } from "@tauri-apps/api/event";
import { editor } from "$lib/editor/editor.svelte";
import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
import { proofing } from "$lib/gpu/proofing.svelte";
import { loadSettings, saveSetting } from "$lib/settings/persistence";
import { defaultExportSettings, restoreExportSettings, type ExportSettings } from "$lib/types/exportSettings";
import {
    exportDefaults,
    exportImage,
    pickExportFile,
    pickExportFolder,
    type ExportDestination,
} from "$lib/utils/exportImage";

export type ExportPhase = "preparing" | "rendering" | "encoding";

export interface ExportProgress {
    phase: ExportPhase;
    progress: number;
    indeterminate: boolean;
}

class ExportDialog {
    open = $state(false);
    settings = $state<ExportSettings>({ ...defaultExportSettings });
    folder = $state("");
    /** Without its extension, which the format decides. */
    fileName = $state("");

    running = $state(false);
    progress = $state<ExportProgress | null>(null);
    error = $state<string | null>(null);
    /** Whether the running export came from the save dialog rather than the modal. */
    quick = $state(false);

    // The open the name was taken from, so the next image brings its own.
    #namedFor: number | null = null;

    readonly hydrated: Promise<void> = loadSettings().then((stored) => {
        if (stored.exportSettings !== undefined) this.settings = restoreExportSettings(stored.exportSettings);
        if (stored.exportFolder) this.folder = stored.exportFolder;
    });

    constructor() {
        $effect.root(() => {
            // What a soft proof proofs against is whatever the export is set to write.
            $effect(() => {
                proofing.target = this.settings.outputSpace;
            });
        });
    }

    show = async () => {
        const image = editor.imagePayload;
        if (!image || this.open) return;
        if (editor.isCropping) editor.applyCrop();
        this.error = null;
        this.open = true;

        if (this.#namedFor !== image.imageId) {
            this.#namedFor = image.imageId;
            const defaults = await exportDefaults().catch(() => null);
            this.fileName = defaults?.fileName ?? "Untitled";
            if (!this.folder && defaults) this.folder = defaults.folder;
        }
    };

    close = () => {
        if (this.running) return;
        this.open = false;
        void saveSetting("exportSettings", $state.snapshot(this.settings));
    };

    toggle = () => {
        if (this.open) this.close();
        else void this.show();
    };

    chooseFolder = async () => {
        const folder = await pickExportFolder(this.folder || null).catch(() => null);
        if (folder) this.folder = folder;
    };

    /** The save dialog, then an export with the settings as they stand. */
    quickExport = async () => {
        if (this.running || this.open || !editor.imagePayload) return;
        if (editor.isCropping) editor.applyCrop();
        const destination = await pickExportFile(this.settings.format, this.folder || null).catch((e) => {
            console.error("Failed to open the save dialog:", e);
            return null;
        });
        if (!destination) return;
        this.quick = true;
        try {
            await this.#export(destination);
        } catch (e) {
            console.error("Failed to export:", e);
        } finally {
            this.quick = false;
        }
    };

    /** The export the modal is set up for; a failure stays on the modal. */
    run = async () => {
        if (this.running) return;
        try {
            await this.#export({ folder: this.folder, fileName: this.fileName });
            this.close();
        } catch (e) {
            this.error = String(e);
        }
    };

    async #export(destination: ExportDestination) {
        if (this.running || !editor.imagePayload) return;
        this.running = true;
        this.error = null;
        this.progress = { phase: "preparing", progress: 0, indeterminate: true };
        // Greyscale output follows the mode the image is edited in.
        this.settings.isBw = !editor.edit.isRgb;
        // A focused control would otherwise keep taking keys while it runs.
        (document.activeElement as HTMLElement)?.blur();

        const unlisten = await listen<ExportProgress>("export:progress", (e) => {
            this.progress = e.payload;
        });
        try {
            await exportImage(editor.edit, editor.curveLut, colorSpaceDebug.toneMap, $state.snapshot(this.settings), destination);
            this.folder = destination.folder;
            void saveSetting("exportFolder", destination.folder);
        } finally {
            unlisten();
            this.running = false;
            this.progress = null;
        }
    }
}

export const exportDialog = new ExportDialog();
