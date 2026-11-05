// src/lib/gpu/retouchLayer.ts
//
// Bakes the heal and clone strokes into the source texture itself, so the
// develop chain, the backdrop and the loupe all read the retouched image and
// moving the view never touches a stroke. The texels a stroke overwrites are
// backed up the first time, so a stroke undone or changed can be replayed from
// the untouched image.

import type { GPUImage } from "$lib/types/gpuTypes";
import {
    RETOUCH_TILE,
    RETOUCH_UNIFORM_SIZE,
    placeRetouch,
    retouchMargin,
    retouchTile,
    retouchTiles,
    strokeSegments,
    tileReads,
    retouchUniform,
    segmentsBuffer,
    cloneRetouch,
    pointsStartWith,
    retouchEqual,
    sameBrush,
    type PlacedRetouch,
    type Retouch,
    type RetouchTile,
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

/** Texels of blur textures kept between updates; larger ones are freed once used. */
const KEEP_WORK_TEXELS = 768 * 768;

export interface RetouchLayer {
    /** Brings `image` in line with `strokes`, applying only those newly added. Returns whether it changed. */
    sync(image: GPUImage, strokes: readonly Retouch[]): boolean;
    /** Paints `stroke` while it is drawn, kept once synced; null takes it away. Returns whether it changed. */
    paint(image: GPUImage, stroke: Retouch | null): boolean;
    destroy(): void;
}

/** The stroke being drawn, with the image under it as it was before. */
interface Live {
    stroke: Retouch;
    // What each tile it reaches renders to, by tile.
    tiles: Map<number, { tile: RetouchTile; output: GPUTexture }>;
    // Blocks of the image from before the stroke, by backup block.
    before: Map<number, GPUTexture>;
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
    let live: Live | null = null;

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

    function releaseLargeWork() {
        if (workSize.width * workSize.height <= KEEP_WORK_TEXELS) return;
        for (const t of work) t.destroy();
        work = [];
        workSize = { width: 0, height: 0 };
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

    /** Copies the blocks of `box` not yet in `into`, as the image holds them now. */
    function backUp(encoder: GPUCommandEncoder, box: TexelBox, into = backups) {
        backupTiles(box, (key, x, y) => {
            if (into.has(key)) return;
            const size = backupSize(x, y);
            const copy = device.createTexture({
                label: "Retouch Backup",
                size,
                format: SOURCE_FORMAT,
                usage: GPUTextureUsage.COPY_DST | GPUTextureUsage.COPY_SRC,
            });
            encoder.copyTextureToTexture({ texture: image!.texture, origin: { x, y } }, { texture: copy }, size);
            into.set(key, copy);
        });
    }

    function blockBox(key: number): TexelBox {
        const columns = Math.ceil(image!.width / BACKUP_TILE);
        const x = (key % columns) * BACKUP_TILE;
        const y = Math.floor(key / columns) * BACKUP_TILE;
        const size = backupSize(x, y);
        return [x, y, x + size.width, y + size.height];
    }

    /** Writes the blocks of `from` back, all of them or only those `keys` names. */
    function restore(encoder: GPUCommandEncoder, from = backups, keys: Iterable<number> = from.keys()): TexelBox[] {
        const restored: TexelBox[] = [];
        for (const key of keys) {
            const copy = from.get(key)!;
            const box = blockBox(key);
            encoder.copyTextureToTexture(
                { texture: copy },
                { texture: image!.texture, origin: { x: box[0], y: box[1] } },
                { width: box[2] - box[0], height: box[3] - box[1] },
            );
            restored.push(box);
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

    function fitTiles(placed: PlacedRetouch, tiles: RetouchTile[]) {
        if (placed.mode !== "heal") return;
        for (const { work } of tiles) fitWork(work[2] - work[0], work[3] - work[1]);
    }

    /** Records one tile of a stroke into `output`, which is the size of the tile's target. */
    function renderTile(
        encoder: GPUCommandEncoder,
        placed: PlacedRetouch,
        tile: RetouchTile,
        output: GPUTexture,
        transient: { destroy(): void }[],
    ) {
        const { width, height } = image!;
        const uniform = device.createBuffer({
            size: RETOUCH_UNIFORM_SIZE,
            usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
        });
        device.queue.writeBuffer(uniform, 0, retouchUniform(placed, tile, [0, 0], [0, 0], width, height));
        const segmentData = segmentsBuffer(tile);
        const segments = device.createBuffer({
            size: segmentData.byteLength,
            usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
        });
        device.queue.writeBuffer(segments, 0, segmentData);
        transient.push(uniform, segments);

        const imageView = image!.texture.createView();
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
        if (placed.mode === "heal") {
            const ww = tile.work[2] - tile.work[0];
            const wh = tile.work[3] - tile.work[1];
            runPass(encoder, prep, work[0], ww, wh, bind(blank));
            runPass(encoder, blurH, work[1], ww, wh, bind(work[0]));
            runPass(encoder, blurV, work[2], ww, wh, bind(work[1]));
            matched = work[2];
        }
        runPass(encoder, apply, output, tile.target[2] - tile.target[0], tile.target[3] - tile.target[1], bind(matched));
    }

    function tileOutput(tile: RetouchTile): GPUTexture {
        return device.createTexture({
            label: "Retouch Tile",
            size: { width: tile.target[2] - tile.target[0], height: tile.target[3] - tile.target[1] },
            format: SOURCE_FORMAT,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
        });
    }

    function writeTile(encoder: GPUCommandEncoder, tile: RetouchTile, output: GPUTexture) {
        const [x0, y0, x1, y1] = tile.target;
        encoder.copyTextureToTexture(
            { texture: output },
            { texture: image!.texture, origin: { x: x0, y: y0 } },
            { width: x1 - x0, height: y1 - y0 },
        );
    }

    // Every tile reads the image as it was before the stroke, so the tiles are
    // all rendered before any of them is written back.
    function applyStroke(encoder: GPUCommandEncoder, stroke: Retouch, transient: { destroy(): void }[]): TexelBox[] {
        const placed = placeRetouch(stroke, image!.width, image!.height);
        const written: { tile: RetouchTile; output: GPUTexture }[] = [];
        for (const tile of retouchTiles(placed, image!.width, image!.height)) {
            const output = tileOutput(tile);
            transient.push(output);
            renderTile(encoder, placed, tile, output, transient);
            written.push({ tile, output });
        }
        for (const { tile, output } of written) {
            backUp(encoder, tile.target);
            writeTile(encoder, tile, output);
        }
        return written.map((w) => w.tile.target);
    }

    // ── The stroke being drawn ────────────────────────────────────────────────
    // Laid on a fixed grid of tiles, so a tile it has already rendered stays
    // as it is until a new segment of the stroke reaches it.

    /** Whether `next` is `stroke` with points added, or the same. */
    function continues(stroke: Retouch, next: Retouch): boolean {
        return sameBrush(stroke, next) && pointsStartWith(next, stroke);
    }

    function dropLive(encoder: GPUCommandEncoder, transient: { destroy(): void }[]): TexelBox[] {
        if (!live) return [];
        const restored = restore(encoder, live.before);
        releaseLive(transient);
        return restored;
    }

    /** Lets the stroke go; its textures go into `into`, to be freed once what reads them is submitted. */
    function releaseLive(into?: { destroy(): void }[]) {
        if (!live) return;
        const textures = [...[...live.tiles.values()].map((t) => t.output), ...live.before.values()];
        if (into) into.push(...textures);
        else for (const t of textures) t.destroy();
        live = null;
    }

    function paintLive(encoder: GPUCommandEncoder, next: Retouch, transient: { destroy(): void }[]): TexelBox[] {
        const { width, height } = image!;
        const placed = placeRetouch(next, width, height);
        const segments = strokeSegments(placed.points);
        const known = live ? live.stroke.points.length : 0;
        if (known === next.points.length) return [];
        if (!live) live = { stroke: next, tiles: new Map(), before: new Map() };
        // Until it has two points the stroke is a spot, whose one segment the next point replaces.
        const fresh = known <= 1 ? 0 : known - 1;
        const added = segments.slice(fresh);
        live.stroke = cloneRetouch(next);
        if (added.length === 0) return [];

        // The cells the new segments can reach, through the tile or its blur.
        const grow = placed.radius + 1 + retouchMargin(placed);
        const columns = Math.ceil(width / RETOUCH_TILE);
        const cells = new Set<number>();
        for (const [ax, ay, bx, by] of added) {
            const cx0 = Math.max(0, Math.floor((Math.min(ax, bx) - grow) / RETOUCH_TILE));
            const cy0 = Math.max(0, Math.floor((Math.min(ay, by) - grow) / RETOUCH_TILE));
            const cx1 = Math.min(columns - 1, Math.floor((Math.max(ax, bx) + grow) / RETOUCH_TILE));
            const cy1 = Math.min(Math.ceil(height / RETOUCH_TILE) - 1, Math.floor((Math.max(ay, by) + grow) / RETOUCH_TILE));
            for (let cy = cy0; cy <= cy1; cy++) for (let cx = cx0; cx <= cx1; cx++) cells.add(cy * columns + cx);
        }

        const dirty: { key: number; tile: RetouchTile }[] = [];
        for (const key of cells) {
            const x = (key % columns) * RETOUCH_TILE;
            const y = Math.floor(key / columns) * RETOUCH_TILE;
            const target: TexelBox = [x, y, Math.min(x + RETOUCH_TILE, width), Math.min(y + RETOUCH_TILE, height)];
            const tile = retouchTile(placed, segments, target, width, height);
            if (!tile) continue;
            const kept = live.tiles.get(key);
            // Old segments stay as they were, so a tile reached by no new one renders the same.
            if (fresh > 0 && kept && kept.tile.segments.length === tile.segments.length) continue;
            dirty.push({ key, tile });
        }
        if (dirty.length === 0) return [];
        fitTiles(placed, dirty.map((d) => d.tile));

        // The dirty tiles read the image from before the stroke.
        const reads = new Set<number>();
        for (const { tile } of dirty) {
            const { work, source } = tileReads(tile, placed.offset, width, height);
            for (const box of source ? [work, source] : [work]) {
                backupTiles(box, (key) => live!.before.has(key) && reads.add(key));
            }
        }
        const restored = restore(encoder, live.before, reads);

        for (const { key, tile } of dirty) {
            let output = live.tiles.get(key)?.output;
            if (!output) {
                output = tileOutput(tile);
                backUp(encoder, tile.target, live.before);
                backUp(encoder, tile.target);
            }
            renderTile(encoder, placed, tile, output, transient);
            live.tiles.set(key, { tile, output });
        }

        // Back over what was restored, from the tiles kept, then the new ones.
        const rewritten = new Set(dirty.map((d) => d.key));
        for (const [key, { tile, output }] of live.tiles) {
            if (rewritten.has(key) || !restored.some((box) => overlaps(box, tile.target))) continue;
            writeTile(encoder, tile, output);
        }
        for (const { key } of dirty) {
            const { tile, output } = live.tiles.get(key)!;
            writeTile(encoder, tile, output);
        }
        return dirty.map((d) => d.tile.target);
    }

    function finishMips(encoder: GPUCommandEncoder, dirty: TexelBox[]) {
        const regions = dirty.length > MAX_MIP_REGIONS ? [union(dirty)] : dirty;
        for (const box of regions) recordMipmaps(device, encoder, image!.texture, box);
    }

    function paint(next: GPUImage, stroke: Retouch | null): boolean {
        if (next !== image) return false;
        if (!live && !stroke) return false;
        const encoder = device.createCommandEncoder({ label: "Retouch Live" });
        const transient: { destroy(): void }[] = [];
        const dirty = live && (!stroke || !continues(live.stroke, stroke)) ? dropLive(encoder, transient) : [];
        if (stroke) dirty.push(...paintLive(encoder, stroke, transient));
        if (dirty.length === 0) return false;
        finishMips(encoder, dirty);
        device.queue.submit([encoder.finish()]);
        for (const t of transient) t.destroy();
        if (!live) releaseLargeWork();
        return true;
    }

    function sync(next: GPUImage, strokes: readonly Retouch[]): boolean {
        if (next !== image) {
            releaseLive();
            clearBackups();
            image = next;
            applied = [];
        }
        const extends_ =
            applied.length <= strokes.length && applied.every((s, i) => retouchEqual(s, strokes[i]));
        if (extends_ && applied.length === strokes.length) return false;

        const encoder = device.createCommandEncoder({ label: "Retouch" });
        const transient: { destroy(): void }[] = [];
        const dirty: TexelBox[] = [];
        let pending = strokes.slice(extends_ ? applied.length : 0);

        // The stroke drawn live is kept when it is the one added, and taken
        // away otherwise, so the strokes are applied over the image without it.
        const adopted = live && extends_ && pending.length === 1 && continues(live.stroke, pending[0]);
        if (adopted) {
            dirty.push(...paintLive(encoder, pending[0], transient));
            pending = [];
            releaseLive(transient);
        } else {
            dirty.push(...dropLive(encoder, transient));
        }

        // Sized for every stroke up front: a texture replaced mid-recording
        // would invalidate the passes already recorded against it.
        for (const stroke of pending) {
            const placed = placeRetouch(stroke, image.width, image.height);
            fitTiles(placed, retouchTiles(placed, image.width, image.height));
        }
        if (!extends_) dirty.push(...restore(encoder));
        for (const stroke of pending) dirty.push(...applyStroke(encoder, stroke, transient));

        finishMips(encoder, dirty);
        device.queue.submit([encoder.finish()]);
        for (const t of transient) t.destroy();
        if (!live) releaseLargeWork();

        applied = strokes.map(cloneRetouch);
        return true;
    }

    function destroy() {
        releaseLive();
        clearBackups();
        for (const t of work) t.destroy();
        blank.destroy();
        image = null;
        applied = [];
    }

    return { sync, paint, destroy };
}

function overlaps(a: TexelBox, b: TexelBox): boolean {
    return a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
}

function union(boxes: TexelBox[]): TexelBox {
    return boxes.reduce((a, b) => [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[2], b[2]), Math.max(a[3], b[3])]);
}
