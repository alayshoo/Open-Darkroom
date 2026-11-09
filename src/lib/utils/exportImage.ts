/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { ExportFormat, ExportSettings } from "$lib/types/exportSettings";
import type { ToneMap } from "$lib/types/toneMap";
import type { ImageEdit } from "$lib/types/imageEdit";
import { cloneRetouch } from "$lib/types/retouch";

export interface ExportDestination {
    folder: string;
    /** The format's extension is added unless the name already carries it. */
    fileName: string;
    /** The name came from the system's save dialog rather than being typed. */
    picked?: boolean;
}

// The curves travel already baked, so the export samples the same table the
// preview does. An empty table means every curve is the identity.
export async function exportImage(
    edit: ImageEdit,
    curveLut: Float32Array | null,
    toneMap: ToneMap,
    settings: ExportSettings,
    destination: ExportDestination,
): Promise<void> {
    const curves = curveLut ? Array.from(curveLut) : [];
    await invoke("export_image", {
        sliders: { ...edit.sliders, toneMap, curveLut: curves },
        crop: { ...edit.crop },
        retouch: edit.retouch.map(cloneRetouch),
        settings,
        destination,
        size: outputSize(settings),
        metadata: settings.metadata,
    });
}

/** The open image's own folder and name, or null with nothing open. */
export function exportDefaults(): Promise<ExportDestination | null> {
    return invoke("export_defaults");
}

/** The save dialog; null when it is dismissed. */
export function pickExportFile(format: ExportFormat, folder: string | null): Promise<ExportDestination | null> {
    return invoke("pick_export_file", { format, folder });
}

/** Null when the picker is dismissed. */
export function pickExportFolder(start: string | null): Promise<string | null> {
    return invoke("pick_export_folder", { start });
}

export interface DestinationCheck {
    path: string;
    /** The name the file is written under, extension included. */
    fileName: string;
    exists: boolean;
}

/** Rejects with the reason when the folder or the name cannot be written to. */
export function checkExportDestination(destination: ExportDestination, format: ExportFormat): Promise<DestinationCheck> {
    return invoke("check_export_destination", { destination, format });
}

export interface OutputDimensions {
    /** The crop as it renders, before any resize. */
    source: [number, number];
    output: [number, number];
}

/** The size the crop renders at and the size the settings write it at; null with nothing open. */
export function exportOutputSize(crop: ImageEdit["crop"], settings: ExportSettings): Promise<OutputDimensions | null> {
    return invoke("export_output_size", { crop: { ...crop }, size: outputSize(settings) });
}

function outputSize(settings: ExportSettings) {
    return { mode: settings.resize, pixels: settings.resizePixels, percent: settings.resizePercent };
}
