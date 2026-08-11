// src/lib/gpu/debugStats.svelte.ts

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

/**
 * What one histogram update cost.
 *
 * Nothing here waits: the bins are drawn on the GPU, so an update records its
 * passes and submits. That leaves two separate costs rather than one total —
 * time on the GPU, which the passes report, and time on the CPU recording them,
 * which finishes before the GPU has started. Adding them would describe no real
 * duration, so they stay apart.
 */
export interface HistogramSample {
    /** Per-pass GPU times, empty when the adapter has no timestamp support. */
    passes: PassTiming[];
    /** Recording the command buffer and submitting it. */
    recordMs: number;
}

/** Roughly three seconds of history at 60fps. */
const CAPACITY = 180;

/** A frame older than this means nothing is being rendered right now. */
export const IDLE_AFTER_MS = 500;

class DebugStats {
    /** Whether the overlay is showing, and so whether the renderer measures at
     *  all — the timestamp queries are not free, so they follow this. */
    enabled = $state(false);
    supportsPassTimings = $state(false);

    /** Diagnostic kill switch for the live histogram. It shares the render
     *  queue, and turnaround waits on the whole queue rather than on the render
     *  alone — so telling the two apart means being able to stop one. */
    histogramEnabled = $state(true);

    /** Bumped once per recorded frame. The samples are a plain array so that
     *  reading the history costs one reactive read rather than one per sample. */
    version = $state(0);
    lastRecordedAt = $state(0);

    readonly samples: FrameSample[] = [];

    /** Histogram updates run on their own cadence, so they are kept apart from
     *  the frame samples rather than folded into whichever frame they landed on. */
    histogramVersion = $state(0);
    readonly histogramSamples: HistogramSample[] = [];

    record(sample: FrameSample) {
        this.samples.push(sample);
        if (this.samples.length > CAPACITY) this.samples.shift();
        this.lastRecordedAt = performance.now();
        this.version++;
    }

    recordHistogram(sample: HistogramSample) {
        this.histogramSamples.push(sample);
        if (this.histogramSamples.length > CAPACITY) this.histogramSamples.shift();
        this.histogramVersion++;
    }

    clear() {
        this.samples.length = 0;
        this.histogramSamples.length = 0;
        this.version++;
        this.histogramVersion++;
    }
}

export const debugStats = new DebugStats();
