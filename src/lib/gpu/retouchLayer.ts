// src/lib/gpu/retouchLayer.ts
//
// Bakes the heal and clone strokes into the source texture itself, so the
// develop chain, the backdrop and the loupe all read the retouched image and
// moving the view never touches a stroke. The texels a stroke overwrites are
// backed up the first time, so a stroke undone or changed can be replayed from
// the untouched image.

import type { GPUImage } from "$lib/types/gpuTypes";
import {
    RETOUCH_UNIFORM_SIZE,
    placeRetouch,
    retouchTiles,
    retouchUniform,
    segmentsBuffer,
    cloneRetouch,
    retouchEqual,
    type Retouch,
    type TexelBox,
} from "$lib/types/retouch";
import { SOURCE_FORMAT, recordMipmaps } from "./fullResolution";
import retouchSource from "./shaders/retouch.wgsl?raw";

/** The blur's textures; 32-bit so the log ratios keep their precision. */
const WORK_FORMAT: GPUTextureFormat = "rgba32float";

/** Texels on each side of the blocks the untouched image is backed up in. */
const BACKUP_TILE = 256;

/** Past this many separate regions the mip levels are rebuilt over their union. */
const MAX_MIP_REGIONS = 16;

export interface RetouchLayer {
    /**
     * Brings `image` in line with `strokes`, applying only those added since
     * the last call when the rest are unchanged. Returns whether it changed.
     */
    sync(image: GPUImage, strokes: readonly Retouch[]): boolean;
    destroy(): void;
}

export function createRetouchLayer(device: GPUDevice): RetouchLayer {
    const textureEntry = (binding: number): GPUBindGroupLayoutEntry => ({
        binding,
        visibility: GPUShaderStage.FRAGMENT,
        texture: { sampleType: "unfilterable-float" },
    });
    const layout = device.createBindGroupLayout({
        label: "Retouch",
        entries: [
            textureEntry(0),
            textureEntry(1),
            { binding: 2, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "uniform" } },
            { binding: 3, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "read-only-storage" } },
        ],
    });
    const module = device.createShaderModule({ label: "Retouch", code: retouchSource });
    const pipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [layout] });
    const pipeline = (entryPoint: string, format: GPUTextureFormat) =>
        device.createRenderPipeline({
            label: `Retouch ${entryPoint}`,
            layout: pipelineLayout,
            vertex: { module, entryPoint: "vs_main" },
            fragment: { module, entryPoint, targets: [{ format }] },
        });
    const prep = pipeline("fs_prep", WORK_FORMAT);
    const blurH = pipeline("fs_blur_h", WORK_FORMAT);
    const blurV = pipeline("fs_blur_v", WORK_FORMAT);
    const apply = pipeline("fs_apply", SOURCE_FORMAT);

    // Stands in for the blurred match in a clone, which never reads it.
    const blank = device.createTexture({
        label: "Retouch Blank",
        size: { width: 1, height: 1 },
        format: WORK_FORMAT,
        usage: GPUTextureUsage.TEXTURE_BINDING,
    });

    let work: GPUTexture[] = [];
    let workSize = { width: 0, height: 0 };

    let image: GPUImage | null = null;
    let applied: readonly Retouch[] = [];
    const backups = new Map<number, GPUTexture>();

    function fitWork(width: number, height: number) {
        if (width <= workSize.width && height <= workSize.height) return;
        for (const t of work) t.destroy();
        workSize = { width: Math.max(width, workSize.width), height: Math.max(height, workSize.height) };
        work = [0, 1, 2].map((i) =>
            device.createTexture({
                label: `Retouch Work ${i}`,
                size: workSize,
                format: WORK_FORMAT,
                usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
            }),
        );
    }

    function clearBackups() {
        for (const t of backups.values()) t.destroy();
        backups.clear();
    }

    function backupTiles(box: TexelBox, each: (key: number, x: number, y: number) => void) {
        const columns = Math.ceil(image!.width / BACKUP_TILE);
        for (let ty = Math.floor(box[1] / BACKUP_TILE); ty * BACKUP_TILE < box[3]; ty++) {
            for (let tx = Math.floor(box[0] / BACKUP_TILE); tx * BACKUP_TILE < box[2]; tx++) {
                each(ty * columns + tx, tx * BACKUP_TILE, ty * BACKUP_TILE);
            }
        }
    }

    function backupSize(x: number, y: number) {
        return {
            width: Math.min(BACKUP_TILE, image!.width - x),
            height: Math.min(BACKUP_TILE, image!.height - y),
        };
    }

    function backUp(encoder: GPUCommandEncoder, box: TexelBox) {
        backupTiles(box, (key, x, y) => {
            if (backups.has(key)) return;
            const size = backupSize(x, y);
            const copy = device.createTexture({
                label: "Retouch Backup",
                size,
                format: SOURCE_FORMAT,
                usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.COPY_SRC,
            });
            encoder.copyTextureToTexture({ texture: image!.texture, origin: { x, y } }, { texture: copy }, size);
            backups.set(key, copy);
        });
    }

    function restore(encoder: GPUCommandEncoder): TexelBox[] {
        const columns = Math.ceil(image!.width / BACKUP_TILE);
        const restored: TexelBox[] = [];
        for (const [key, copy] of backups) {
            const x = (key % columns) * BACKUP_TILE;
            const y = Math.floor(key / columns) * BACKUP_TILE;
            const size = backupSize(x, y);
            encoder.copyTextureToTexture({ texture: copy }, { texture: image!.texture, origin: { x, y } }, size);
            restored.push([x, y, x + size.width, y + size.height]);
        }
        return restored;
    }

    function runPass(
        encoder: GPUCommandEncoder,
        pipeline: GPURenderPipeline,
        target: GPUTexture,
        width: number,
        height: number,
        bindGroup: GPUBindGroup,
    ) {
        const pass = encoder.beginRenderPass({
            colorAttachments: [{ view: target.createView(), loadOp: "clear", storeOp: "store" }],
        });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.setScissorRect(0, 0, width, height);
        pass.draw(6);
        pass.end();
    }

    // Every tile reads the image as it was before the stroke, so the tiles are
    // all rendered before any of them is written back.
    function applyStroke(encoder: GPUCommandEncoder, stroke: Retouch, transient: { destroy(): void }[]): TexelBox[] {
        const { width, height } = image!;
        const placed = placeRetouch(stroke, width, height);
        const heal = placed.mode === "heal";
        const imageView = image!.texture.createView();
        const written: { box: TexelBox; texture: GPUTexture }[] = [];

        for (const tile of retouchTiles(placed, width, height)) {
            const uniform = device.createBuffer({
                size: RETOUCH_UNIFORM_SIZE,
                usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
            });
            device.queue.writeBuffer(uniform, 0, retouchUniform(placed, tile, [0, 0], width, height));
            const segmentData = segmentsBuffer(tile);
            const segments = device.createBuffer({
                size: segmentData.byteLength,
                usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
            });
            device.queue.writeBuffer(segments, 0, segmentData);
            transient.push(uniform, segments);

            const bind = (input: GPUTexture) =>
                device.createBindGroup({
                    layout,
                    entries: [
                        { binding: 0, resource: imageView },
                        { binding: 1, resource: input.createView() },
                        { binding: 2, resource: { buffer: uniform } },
                        { binding: 3, resource: { buffer: segments } },
                    ],
                });

            let matched = blank;
            if (heal) {
                const ww = tile.work[2] - tile.work[0];
                const wh = tile.work[3] - tile.work[1];
                runPass(encoder, prep, work[0], ww, wh, bind(blank));
                runPass(encoder, blurH, work[1], ww, wh, bind(work[0]));
                runPass(encoder, blurV, work[2], ww, wh, bind(work[1]));
                matched = work[2];
            }

            const tw = tile.target[2] - tile.target[0];
            const th = tile.target[3] - tile.target[1];
            const output = device.createTexture({
                label: "Retouch Tile",
                size: { width: tw, height: th },
                format: SOURCE_FORMAT,
                usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
            });
            transient.push(output);
            runPass(encoder, apply, output, tw, th, bind(matched));
            written.push({ box: tile.target, texture: output });
        }

        for (const { box, texture } of written) {
            backUp(encoder, box);
            encoder.copyTextureToTexture(
                { texture },
                { texture: image!.texture, origin: { x: box[0], y: box[1] } },
                { width: box[2] - box[0], height: box[3] - box[1] },
            );
        }
        return written.map((w) => w.box);
    }

    function sync(next: GPUImage, strokes: readonly Retouch[]): boolean {
        if (next !== image) {
            clearBackups();
            image = next;
            applied = [];
        }
        const extends_ =
            applied.length <= strokes.length && applied.every((s, i) => retouchEqual(s, strokes[i]));
        if (extends_ && applied.length === strokes.length) return false;
        const pending = strokes.slice(extends_ ? applied.length : 0);

        // Sized for every stroke up front: a texture replaced mid-recording
        // would invalidate the passes already recorded against it.
        for (const stroke of pending) {
            const placed = placeRetouch(stroke, image.width, image.height);
            if (placed.mode !== "heal") continue;
            for (const { work } of retouchTiles(placed, image.width, image.height)) {
                fitWork(work[2] - work[0], work[3] - work[1]);
            }
        }

        const encoder = device.createCommandEncoder({ label: "Retouch" });
        const transient: { destroy(): void }[] = [];
        const dirty: TexelBox[] = extends_ ? [] : restore(encoder);
        for (const stroke of pending) dirty.push(...applyStroke(encoder, stroke, transient));

        const regions = dirty.length > MAX_MIP_REGIONS ? [union(dirty)] : dirty;
        for (const box of regions) recordMipmaps(device, encoder, image.texture, box);
        device.queue.submit([encoder.finish()]);
        for (const t of transient) t.destroy();

        applied = strokes.map(cloneRetouch);
        return true;
    }

    function destroy() {
        clearBackups();
        for (const t of work) t.destroy();
        blank.destroy();
        image = null;
        applied = [];
    }

    return { sync, destroy };
}

function union(boxes: TexelBox[]): TexelBox {
    return boxes.reduce((a, b) => [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])]);
}
