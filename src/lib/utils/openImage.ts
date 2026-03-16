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

    // Pixel data: width * height * 8 bytes (4 channels × f16)
    const pixelByteLength = width * height * 8;
    const pixels = new Uint8Array(buffer, 8, pixelByteLength);

    // Histogram data: 3 × 256 × 4 bytes appended after pixels
    const histOffset = 8 + pixelByteLength;
    const histR = new Uint32Array(buffer, histOffset, 256);
    const histG = new Uint32Array(buffer, histOffset + 256 * 4, 256);
    const histB = new Uint32Array(buffer, histOffset + 512 * 4, 256);

    return { width, height, pixels, histR, histG, histB };
}
