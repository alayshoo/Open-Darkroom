/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { ExportFormat, ExportSettings } from "$lib/types/exportSettings";
import type { ToneMap } from "$lib/types/toneMap";
import type { ImageEdit } from "$lib/types/imageEdit";
import { cloneRetouch } from "$lib/types/retouch";

export interface ExportDestination {
    folder: string;
    /** Without its extension, which the format decides. */
    fileName: string;
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
        size: { mode: settings.resize, pixels: settings.resizePixels, percent: settings.resizePercent },
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
    exists: boolean;
}

/** Rejects with the reason when the folder or the name cannot be written to. */
export function checkExportDestination(destination: ExportDestination, format: ExportFormat): Promise<DestinationCheck> {
    return invoke("check_export_destination", { ...destination, format });
}
