// src/lib/gpu/fullResolution.ts
//
// Streams the open image at full resolution into one mipmapped texture, a band
// of rows at a time, so zooming and panning never have to go back to the
// backend. Mirrors full_resolution.rs.

import type { GPUImage } from "$lib/types/gpuTypes";
import mipmapSource from "./shaders/mipmap.wgsl?raw";

// Must match ROWS_HEADER_BYTES in full_resolution.rs.
export const ROWS_HEADER_BYTES = 16;

const BYTES_PER_PIXEL = 8;

/** Roughly how much one band carries, so no single transfer grows unbounded. */
const BAND_BYTES = 32 * 1024 * 1024;

export const SOURCE_FORMAT: GPUTextureFormat = "rgba16float";

export interface RowsBand {
    // The size of the image being served, which is the full image unless it is
    // past the GPU's texture limit.
    width: number;
    height: number;
    y: number;
    rows: number;
    pixels: Uint8Array;
}

export function parseRows(buffer: ArrayBuffer): RowsBand {
    const view = new DataView(buffer);
    const [width, height, y, rows] = [0, 4, 8, 12].map((o) => view.getUint32(o, true));
    const pixels = new Uint8Array(buffer, ROWS_HEADER_BYTES, width * rows * BYTES_PER_PIXEL);
    return { width, height, y, rows, pixels };
}

/** Rows per band for an image `width` pixels wide. */
export function bandRows(width: number): number {
    return Math.max(1, Math.floor(BAND_BYTES / (width * BYTES_PER_PIXEL)));
}

export function mipLevelCount(width: number, height: number): number {
    return Math.floor(Math.log2(Math.max(width, height))) + 1;
}

export type FetchRows = (maxEdge: number, y: number, rows: number) => Promise<ArrayBuffer>;

/**
 * The whole image on the GPU with every mip level filled, or null if it was
 * cancelled or the GPU could not hold it. One band is always in flight while
 * the previous one uploads.
 */
export async function loadFullResolution(
    device: GPUDevice,
    fullWidth: number,
    fetchRows: FetchRows,
    cancelled: () => boolean,
    progress?: (rows: number, height: number) => void,
): Promise<GPUImage | null> {
    const maxEdge = device.limits.maxTextureDimension2D;
    const rows = bandRows(Math.min(fullWidth, maxEdge));
    let texture: GPUTexture | null = null;

    try {
        let next = fetchRows(maxEdge, 0, rows);
        for (;;) {
            const band = parseRows(await next);
            if (cancelled()) {
                texture?.destroy();
                return null;
            }

            if (!texture) {
                texture = await createSourceTexture(device, band.width, band.height);
                if (!texture) return null;
            }

            const end = band.y + band.rows;
            if (end < band.height) next = fetchRows(maxEdge, end, rows);

            device.queue.writeTexture(
                { texture, origin: { x: 0, y: band.y } },
                band.pixels,
                { bytesPerRow: band.width * BYTES_PER_PIXEL, rowsPerImage: band.rows },
                { width: band.width, height: band.rows },
            );
            progress?.(end, band.height);
            if (end >= band.height) break;
        }
    } catch (e) {
        texture?.destroy();
        throw e;
    }
    if (!texture) return null;

    generateMipmaps(device, texture);
    return { texture, width: texture.width, height: texture.height };
}

function sourceTexture(device: GPUDevice, label: string, width: number, height: number): GPUTexture {
    return device.createTexture({
        label,
        size: { width, height },
        format: SOURCE_FORMAT,
        mipLevelCount: mipLevelCount(width, height),
        // Copied out only by the browser tests, which read the levels back.
        usage:
            GPUTextureUsage.TEXTURE_BINDING |
            GPUTextureUsage.COPY_DST |
            GPUTextureUsage.COPY_SRC |
            GPUTextureUsage.RENDER_ATTACHMENT,
    });
}

/** Linear RGBA f16 pixels, as the open payload's preview carries them, with mip levels. */
export function uploadSource(device: GPUDevice, pixels: Uint8Array, width: number, height: number): GPUImage {
    const texture = sourceTexture(device, "Preview Source", width, height);
    device.queue.writeTexture(
        { texture },
        pixels,
        { bytesPerRow: width * BYTES_PER_PIXEL, rowsPerImage: height },
        { width, height },
    );
    generateMipmaps(device, texture);
    return { texture, width, height };
}

async function createSourceTexture(device: GPUDevice, width: number, height: number) {
    device.pushErrorScope("out-of-memory");
    const texture = sourceTexture(device, "Full Resolution Source", width, height);
    const error = await device.popErrorScope();
    if (error) {
        console.error("Not enough GPU memory for the full-resolution image:", error.message);
        texture.destroy();
        return null;
    }
    return texture;
}

const mipmapPipelines = new WeakMap<GPUDevice, { pipeline: GPURenderPipeline; sampler: GPUSampler }>();

/** Fills every level of `texture` below the first from the one above it. */
export function generateMipmaps(device: GPUDevice, texture: GPUTexture) {
    let cached = mipmapPipelines.get(device);
    if (!cached) {
        const module = device.createShaderModule({ label: "Mipmap Shader", code: mipmapSource });
        cached = {
            pipeline: device.createRenderPipeline({
                label: "Mipmap Pipeline",
                layout: "auto",
                vertex: { module, entryPoint: "vs_main" },
                fragment: { module, entryPoint: "fs_main", targets: [{ format: SOURCE_FORMAT }] },
            }),
            sampler: device.createSampler({
                label: "Mipmap Sampler",
                magFilter: "linear",
                minFilter: "linear",
                addressModeU: "clamp-to-edge",
                addressModeV: "clamp-to-edge",
            }),
        };
        mipmapPipelines.set(device, cached);
    }
    const { pipeline, sampler } = cached;

    const encoder = device.createCommandEncoder({ label: "Generate Mipmaps" });
    for (let level = 1; level < texture.mipLevelCount; level++) {
        const bindGroup = device.createBindGroup({
            layout: pipeline.getBindGroupLayout(0),
            entries: [
                { binding: 0, resource: texture.createView({ baseMipLevel: level - 1, mipLevelCount: 1 }) },
                { binding: 1, resource: sampler },
            ],
        });
        const pass = encoder.beginRenderPass({
            colorAttachments: [
                {
                    view: texture.createView({ baseMipLevel: level, mipLevelCount: 1 }),
                    loadOp: "clear",
                    storeOp: "store",
                },
            ],
        });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.draw(6);
        pass.end();
    }
    device.queue.submit([encoder.finish()]);
}
