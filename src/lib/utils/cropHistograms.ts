/* $lib/utils/cropHistograms.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Crop } from "$lib/types/crop";

export interface ChannelHistograms {
    r: Uint32Array;
    g: Uint32Array;
    b: Uint32Array;
}

// The open image's per-channel histograms over only what the crop keeps, laid
// out as the open payload's histogram tail is.
export async function cropHistograms(crop: Crop): Promise<ChannelHistograms> {
    const buffer = await invoke<ArrayBuffer>("crop_histograms", { crop: { ...crop } });
    return {
        r: new Uint32Array(buffer, 0, 256),
        g: new Uint32Array(buffer, 256 * 4, 256),
        b: new Uint32Array(buffer, 512 * 4, 256),
    };
}
