/* $lib/utils/exportImage.ts */

import { invoke } from "@tauri-apps/api/core";
import type { ExportSettings } from "$lib/types/exportSettings";
import type { ToneMap } from "$lib/types/toneMap";
import type { ImageEdit } from "$lib/types/imageEdit";
import { cloneRetouch } from "$lib/types/retouch";

// The curves travel already baked, so the export samples the same table the
// preview does. An empty table means every curve is the identity.
export async function exportImage(
    edit: ImageEdit,
    curveLut: Float32Array | null,
    toneMap: ToneMap,
    settings: ExportSettings,
): Promise<void> {
    const curves = curveLut ? Array.from(curveLut) : [];
    await invoke("export_image", {
        sliders: { ...edit.sliders, toneMap, curveLut: curves },
        crop: { ...edit.crop },
        retouch: edit.retouch.map(cloneRetouch),
        settings,
    });
}
