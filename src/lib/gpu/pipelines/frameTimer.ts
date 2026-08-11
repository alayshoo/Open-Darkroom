// src/lib/gpu/pipelines/frameTimer.ts
//
// Per-pass GPU timings. Each pass writes a timestamp at its start and another at
// its end; after the last one they are resolved into a buffer and read back.
//
// Slots are handed out in the order passes are recorded, so a frame that skips
// the blur simply produces a shorter list rather than a gap.

import type { GPUSession } from "$lib/types/gpuTypes";
import type { PassTiming } from "../debugStats.svelte";

/** Enough for the develop chain with room to grow. Two timestamps each. */
const MAX_PASSES = 12;
const BYTES = MAX_PASSES * 2 * 8;

export interface FrameTimer {
    /** False when the adapter has no timestamp support; every method is then a
     *  no-op and `read` resolves empty. */
    available: boolean;
    /** Start a new frame, releasing the slots the previous one held. */
    beginFrame(): void;
    /** Claim a slot pair for one pass, to be spread into its descriptor. */
    passWrites(label: string): GPURenderPassTimestampWrites | undefined;
    /** Record the resolve and the copy out. Call after the frame's last pass. */
    resolve(encoder: GPUCommandEncoder): void;
    /** Read back what the frame measured. */
    read(): Promise<PassTiming[]>;
    destroy(): void;
}

export function createFrameTimer(gpu: GPUSession): FrameTimer {
    if (!gpu.canTimestamp) return createDisabledTimer();

    const { device } = gpu;

    const querySet = device.createQuerySet({
        label: "Frame Timer Query Set",
        type: "timestamp",
        count: MAX_PASSES * 2,
    });

    const resolveBuffer = device.createBuffer({
        label: "Frame Timer Resolve Buffer",
        size: BYTES,
        usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_SRC,
    });

    const readBuffer = device.createBuffer({
        label: "Frame Timer Read Buffer",
        size: BYTES,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });

    let labels: string[] = [];
    let resolved: string[] = [];
    // Copying into a mapped buffer is invalid, so a frame recorded while the
    // previous read is outstanding goes untimed rather than skipping the read.
    let mapPending = false;
    let destroyed = false;

    function beginFrame() {
        labels = [];
    }

    function passWrites(label: string): GPURenderPassTimestampWrites | undefined {
        if (mapPending || labels.length >= MAX_PASSES) return undefined;
        const slot = labels.length;
        labels.push(label);
        return {
            querySet,
            beginningOfPassWriteIndex: slot * 2,
            endOfPassWriteIndex: slot * 2 + 1,
        };
    }

    function resolve(encoder: GPUCommandEncoder) {
        resolved = [];
        if (mapPending || labels.length === 0) return;

        const count = labels.length * 2;
        encoder.resolveQuerySet(querySet, 0, count, resolveBuffer, 0);
        encoder.copyBufferToBuffer(resolveBuffer, 0, readBuffer, 0, count * 8);
        resolved = labels.slice();
    }

    async function read(): Promise<PassTiming[]> {
        if (destroyed || mapPending || resolved.length === 0) return [];

        const recorded = resolved;
        mapPending = true;
        try {
            await readBuffer.mapAsync(GPUMapMode.READ, 0, recorded.length * 16);
            if (destroyed) return [];

            const times = new BigUint64Array(
                readBuffer.getMappedRange(0, recorded.length * 16),
            ).slice();
            readBuffer.unmap();

            return recorded.map((label, i) => {
                const start = times[i * 2];
                const end = times[i * 2 + 1];
                // A pass whose slots were never written reads back as zero.
                const ns = end > start ? Number(end - start) : 0;
                return { label, ms: ns / 1e6 };
            });
        } catch {
            return [];
        } finally {
            mapPending = false;
        }
    }

    function destroy() {
        destroyed = true;
        querySet.destroy();
        resolveBuffer.destroy();
        readBuffer.destroy();
    }

    return { available: true, beginFrame, passWrites, resolve, read, destroy };
}

function createDisabledTimer(): FrameTimer {
    return {
        available: false,
        beginFrame: () => {},
        passWrites: () => undefined,
        resolve: () => {},
        read: async () => [],
        destroy: () => {},
    };
}
