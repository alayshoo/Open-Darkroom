/* $lib/utils/cropHistograms.ts */

import { invoke } from "@tauri-apps/api/core";
import type { Crop } from "$lib/types/crop";
import { cloneRetouch, type Retouch } from "$lib/types/retouch";

export interface ChannelHistograms {
    r: Uint32Array;
    g: Uint32Array;
    b: Uint32Array;
}

// The open image's per-channel histograms over only what the crop keeps, with
// the strokes laid over it, laid out as the open payload's histogram tail is.
export async function cropHistograms(crop: Crop, retouch: Retouch[] = []): Promise<ChannelHistograms> {
    const buffer = await invoke<ArrayBuffer>("crop_histograms", {
        crop: { ...crop },
        retouch: retouch.map(cloneRetouch),
    });
    return {
        r: new Uint32Array(buffer, 0, 256),
        g: new Uint32Array(buffer, 256 * 4, 256),
        b: new Uint32Array(buffer, 512 * 4, 256),
    };
}
