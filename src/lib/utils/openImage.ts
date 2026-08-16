/* $lib/utils/openImage.ts */

import { invoke } from "@tauri-apps/api/core";
import { type ImagePayload } from "$lib/types/imagePayload";


// Returned by the backend when the file picker is dismissed. Must match
// OPEN_CANCELLED in image_opening.rs.
const OPEN_CANCELLED = "open:cancelled";

// Open an image, or return null if the user dismissed the picker. Any other
// rejection means the backend got as far as releasing the image it held, so the
// caller has to drop what it is showing rather than keep it.
export async function openImage(): Promise<ImagePayload | null> {
    let buffer: ArrayBuffer;
    try {
        buffer = await invoke<ArrayBuffer>("open_image_file");
    } catch (e) {
        if (e === OPEN_CANCELLED) return null;
        throw e;
    }

    // Parse the header: preview width and height, then the full-resolution
    // width and height, u32 little-endian.
    const header = new DataView(buffer);
    const width = header.getUint32(0, true);
    const height = header.getUint32(4, true);
    const fullWidth = header.getUint32(8, true);
    const fullHeight = header.getUint32(12, true);

    // Pixel data: width * height * 8 bytes (4 channels × f16)
    const pixelByteLength = width * height * 8;
    const pixels = new Uint8Array(buffer, 16, pixelByteLength);

    // Histogram data: 3 × 256 × 4 bytes appended after pixels
    const histOffset = 16 + pixelByteLength;
    const histR = new Uint32Array(buffer, histOffset, 256);
    const histG = new Uint32Array(buffer, histOffset + 256 * 4, 256);
    const histB = new Uint32Array(buffer, histOffset + 512 * 4, 256);

    return { width, height, fullWidth, fullHeight, pixels, histR, histG, histB };
}
