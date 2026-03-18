/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Sliders } from "$lib/types/imgParameters";
import type { ExportSettings } from "$lib/types/exportSettings";

export async function exportImage(sliders: Sliders, settings: ExportSettings): Promise<void> {
    await invoke("export_image", { sliders, settings });
}
