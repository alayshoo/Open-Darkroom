/* $lib/utils/openImage.ts */

import { invoke } from "@tauri-apps/api/core";
import { type ImagePayload } from "$lib/types/imagePayload";


// Replace the invoke call with this helper
export async function openImage(): Promise<ImagePayload> {
    const buffer = await invoke<ArrayBuffer>("open_image_file");
    
    // Parse the header: first 4 bytes = width, next 4 = height
    const header = new DataView(buffer);
    const width = header.getUint32(0, true);  // little-endian
    const height = header.getUint32(4, true);
    
    // The rest is raw pixel data
    const pixels = new Uint8Array(buffer, 8);
    
    return { width, height, pixels };
}