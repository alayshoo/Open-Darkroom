/* $lib/utils/openImage.ts */

import { invoke } from "@tauri-apps/api/core";
import { type ImagePayload } from "$lib/types/imagePayload";


// Returned by the backend when the file picker is dismissed. Must match
// OPEN_CANCELLED in image_opening.rs.
const OPEN_CANCELLED = "open:cancelled";

// Split the backend payload into its regions. The pixels and histograms are
// views onto the payload, not copies, so they carry a byte offset — anything
// handing them to WebGPU has to pass the view rather than its buffer.
export function parseImagePayload(buffer: ArrayBuffer): ImagePayload {
    // Parse the header: preview width and height, the full-resolution width and
    // height, then the overview width and height, u32 little-endian.
    const header = new DataView(buffer);
    const width = header.getUint32(0, true);
    const height = header.getUint32(4, true);
    const fullWidth = header.getUint32(8, true);
    const fullHeight = header.getUint32(12, true);
    const overviewWidth = header.getUint32(16, true);
    const overviewHeight = header.getUint32(20, true);

    // Pixel data: width * height * 8 bytes (4 channels × f16), preview first
    // and the overview straight after it.
    const pixelByteLength = width * height * 8;
    const pixels = new Uint8Array(buffer, 24, pixelByteLength);

    const overviewOffset = 24 + pixelByteLength;
    const overviewByteLength = overviewWidth * overviewHeight * 8;
    const overviewPixels = new Uint8Array(buffer, overviewOffset, overviewByteLength);

    // Histogram data: 3 × 256 × 4 bytes appended after the pixels
    const histOffset = overviewOffset + overviewByteLength;
    const histR = new Uint32Array(buffer, histOffset, 256);
    const histG = new Uint32Array(buffer, histOffset + 256 * 4, 256);
    const histB = new Uint32Array(buffer, histOffset + 512 * 4, 256);

    return {
        width,
        height,
        fullWidth,
        fullHeight,
        pixels,
        overviewWidth,
        overviewHeight,
        overviewPixels,
        histR,
        histG,
        histB,
    };
}

// Open an image, or return null if the user dismissed the picker. Any other
// rejection means the backend got as far as releasing the image it held, so the
// caller has to drop what it is showing rather than keep it.
export async function openImage(): Promise<ImagePayload | null> {
    try {
        return parseImagePayload(await invoke<ArrayBuffer>("open_image_file"));
    } catch (e) {
        if (e === OPEN_CANCELLED) return null;
        throw e;
    }
}
