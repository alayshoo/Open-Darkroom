/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Sliders } from "$lib/types/imgParameters";

export async function exportImage(sliders: Sliders): Promise<void> {
    await invoke("export_image", { sliders });
}
