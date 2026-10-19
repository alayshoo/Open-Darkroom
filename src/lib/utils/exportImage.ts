/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Sliders } from "$lib/types/imgParameters";
import type { ExportSettings } from "$lib/types/exportSettings";
import type { ToneMap } from "$lib/types/toneMap";

export async function exportImage(
    sliders: Sliders,
    toneMap: ToneMap,
    settings: ExportSettings,
): Promise<void> {
    await invoke("export_image", { sliders: { ...sliders, toneMap }, settings });
}
