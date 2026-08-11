// src/lib/gpu/frameStats.svelte.ts

/** GPU time for a single named pass, as the timestamp queries measured it. */
export interface PassTiming {
    label: string;
    ms: number;
}

/** What one rendered frame cost, split by where the time went. */
export interface FrameSample {
    /** Wall time since the previous rendered frame. */
    intervalMs: number;
    /** Acquiring the canvas texture. Anything above noise means the caller is
     *  still waiting on presentation rather than on the GPU. */
    acquireMs: number;
    /** Recording the command buffer on the CPU. */
    recordMs: number;
    /** Submit until the queue reported the work drained. Not GPU time — it also
     *  covers presentation, vsync and callback dispatch, so it runs well above
     *  the sum of the pass timings. */
    turnaroundMs: number;
    /** Per-pass GPU times, empty when the adapter has no timestamp support. */
    passes: PassTiming[];
}

/** Roughly three seconds of history at 60fps. */
const CAPACITY = 180;

/** A frame older than this means nothing is being rendered right now. */
export const IDLE_AFTER_MS = 500;

class FrameStats {
    /** Whether the overlay is showing, and so whether the renderer measures at
     *  all — the timestamp queries are not free, so they follow this. */
    enabled = $state(false);
    supportsPassTimings = $state(false);

    /** Bumped once per recorded frame. The samples are a plain array so that
     *  reading the history costs one reactive read rather than one per sample. */
    version = $state(0);
    lastRecordedAt = $state(0);

    readonly samples: FrameSample[] = [];

    record(sample: FrameSample) {
        this.samples.push(sample);
        if (this.samples.length > CAPACITY) this.samples.shift();
        this.lastRecordedAt = performance.now();
        this.version++;
    }

    clear() {
        this.samples.length = 0;
        this.version++;
    }
}

export const frameStats = new FrameStats();
