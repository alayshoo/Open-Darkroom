/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Sliders } from "$lib/types/imgParameters";
import type { ExportSettings } from "$lib/types/exportSettings";
import type { ToneMap } from "$lib/types/toneMap";

// The curves travel already baked, so the export samples the same table the
// preview does. An empty table means every curve is the identity.
export async function exportImage(
    sliders: Sliders,
    curveLut: Float32Array | null,
    toneMap: ToneMap,
    settings: ExportSettings,
): Promise<void> {
    const curves = curveLut ? Array.from(curveLut) : [];
    await invoke("export_image", { sliders: { ...sliders, toneMap, curveLut: curves }, settings });
}
