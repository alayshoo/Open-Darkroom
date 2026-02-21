// src/lib/gpu/texture.ts

import type { GPUImage } from "$lib/types/gpuTypes";

// Creates a GPU texture from an ImageBitmap.
// In your app, the Rust backend will decode the RAW file and send
// pixel data to the frontend. You'd convert that into an ImageBitmap
// or use the raw pixel buffer directly.
export async function uploadImageToGPU(
    device: GPUDevice,
    source: ImageBitmap
): Promise<GPUImage> {
    const { width, height } = source;

    // Step 1: Create an empty GPU texture with the right dimensions
    const texture = device.createTexture({
        label: "Image Texture",
        size: { width, height },
        format: "rgba8unorm", // 8 bits per channel, normalized to [0,1]
        usage:
            GPUTextureUsage.TEXTURE_BINDING | // Can be read in shaders
            GPUTextureUsage.COPY_DST |        // Can receive data uploads
            GPUTextureUsage.RENDER_ATTACHMENT, // Can be rendered to
    });

    // Step 2: Copy the ImageBitmap into the texture.
    // This is the most convenient method for browser image sources.
    device.queue.copyExternalImageToTexture(
        { source },              // from
        { texture },             // to
        { width, height }        // size
    );

    return { texture, width, height };
}

// Alternative: upload raw pixel data (e.g., from Rust via Tauri).
// This is what you'll likely use for RAW files.
export function uploadRawPixelsToGPU(
    device: GPUDevice,
    pixels: Uint8Array, // RGBA pixel data
    width: number,
    height: number
): GPUImage {
    const texture = device.createTexture({
        label: "RAW Image Texture",
        size: { width, height },
        format: "rgba8unorm",
        usage:
            GPUTextureUsage.TEXTURE_BINDING |
            GPUTextureUsage.COPY_DST,
    });

    // writeTexture copies a CPU byte array → GPU texture
    device.queue.writeTexture(
        { texture },
        pixels.buffer instanceof SharedArrayBuffer
            ? new Uint8Array(pixels).buffer as ArrayBuffer
            : pixels.buffer as ArrayBuffer,
        {
            // "bytes per row" — each row is width × 4 bytes (RGBA)
            bytesPerRow: width * 4,
            rowsPerImage: height,
        },
        { width, height }
    );

    return { texture, width, height };
}

// Clean up GPU memory when done with an image
export function destroyImage(image: GPUImage) {
    image.texture.destroy();
}