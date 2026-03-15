<!-- src/lib/components/Histogram.svelte -->
<script lang="ts">
    import { untrack } from "svelte";
    import { mapSlidersToParameters, type Sliders } from "$lib/types/imgParameters";
    import type { GPUImage, HistogramPipeline } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createHistogramPipeline } from "$lib/gpu/pipelines/histogramPipeline";
    import { type ImagePayload } from "$lib/types/imagePayload";

    let {
        sliders,
        imagePayload,
    }: {
        sliders: Sliders;
        imagePayload: ImagePayload | null;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let histPipeline: HistogramPipeline | null = $state(null);
    let image: GPUImage | null = $state(null);

    // The last histogram data we received — kept so we can re-draw on resize
    // without re-running the GPU compute pass.
    let lastHistData: { r: Uint32Array; g: Uint32Array; b: Uint32Array } | null =
        $state(null);

    // ── Animated scale ceiling ──────────────────────────────────────────────
    // currentCeiling  – what we actually draw with (animates smoothly)
    // targetCeiling   – where we want to end up (recomputed each histogram)
    //
    // Growth:  instant snap (ceiling jumps up so bars never clip)
    // Decay:   slow lerp  (ceiling drifts down, never chases spikes)
    let currentCeiling = 1;
    let targetCeiling = 1;
    let animFrameId: number | null = null;

    const DECAY_RATE = 0.04;  // fraction of gap closed per frame (~60 fps → ~2 s to halve)
    const SNAP_RATIO = 1.05;  // snap up instantly if target exceeds current by >5 %

    function tickCeiling() {
        animFrameId = null;
        if (!lastHistData) return;

        const gap = targetCeiling - currentCeiling;

        if (targetCeiling > currentCeiling * SNAP_RATIO) {
            // Hard snap upward so bars are never clipped
            currentCeiling = targetCeiling;
        } else {
            // Smooth exponential decay in either direction
            currentCeiling += gap * DECAY_RATE;
        }

        drawHistogram(lastHistData);

        // Keep ticking until settled
        if (Math.abs(gap) > currentCeiling * 0.001) {
            animFrameId = requestAnimationFrame(tickCeiling);
        }
    }

    function scheduleCeilingAnimation() {
        if (animFrameId !== null) return; // already in flight
        animFrameId = requestAnimationFrame(tickCeiling);
    }

    // ── ResizeObserver: keep canvas backing-store at native device pixels ──
    $effect(() => {
        if (!canvas) return;

        const ro = new ResizeObserver((entries) => {
            for (const entry of entries) {
                // Prefer devicePixelContentBoxSize (exact physical pixels).
                // Falls back to contentBoxSize × devicePixelRatio.
                let w: number, h: number;
                if (entry.devicePixelContentBoxSize) {
                    w = entry.devicePixelContentBoxSize[0].inlineSize;
                    h = entry.devicePixelContentBoxSize[0].blockSize;
                } else {
                    const dpr = window.devicePixelRatio || 1;
                    w = Math.round(entry.contentBoxSize[0].inlineSize * dpr);
                    h = Math.round(entry.contentBoxSize[0].blockSize * dpr);
                }

                // Only touch the canvas if the size actually changed
                if (canvas.width !== w || canvas.height !== h) {
                    canvas.width = w;
                    canvas.height = h;
                    // Redraw with existing data (resizing clears the canvas)
                    if (lastHistData) drawHistogram(lastHistData);
                }
            }
        });

        // Request device-pixel-level reporting when supported
        try {
            ro.observe(canvas, { box: "device-pixel-content-box" });
        } catch {
            ro.observe(canvas, { box: "content-box" });
        }

        return () => ro.disconnect();
    });

    // Create the histogram pipeline once on mount
    $effect(() => {
        if (!canvas) return;
        histPipeline = createHistogramPipeline(gpu);

        return () => {
            image?.texture.destroy();
            histPipeline?.destroy();
            histPipeline = null;
            image = null;
            // Cancel any in-flight animation
            if (animFrameId !== null) {
                cancelAnimationFrame(animFrameId);
                animFrameId = null;
            }
        };
    });

    // Load image when payload changes
    $effect(() => {
        if (!histPipeline || !imagePayload) return;

        // Clean up previous image (untracked to avoid re-triggering this effect)
        untrack(() => image?.texture.destroy());

        // Reset both ceilings so the new image starts fresh
        currentCeiling = 1;
        targetCeiling = 1;

        // Upload via the raw pixel path (same as PreviewImageCanvas)
        image = uploadRawPixelsToGPU(
            gpu.device,
            imagePayload.pixels,
            imagePayload.width,
            imagePayload.height,
        );
    });

    // Re-compute histogram when adjustments change
    $effect(() => {
        if (!image || !histPipeline) return;
        // Read adjustments to establish Svelte reactivity tracking
        const _ = { ...sliders };
        updateHistogram();
    });

    let computing = false;
    let pendingUpdate = false;

    async function updateHistogram() {
        if (!image || !histPipeline) return;

        // If a compute is already in flight, flag for re-run when it finishes
        if (computing) {
            pendingUpdate = true;
            return;
        }

        computing = true;
        try {
            const data = await histPipeline.computeHistogram(image.texture, mapSlidersToParameters(sliders));

            // Recompute the target ceiling from the new histogram data.
            // currentCeiling animates toward it — snapping up, decaying down.
            targetCeiling = computeCeiling(data);

            lastHistData = data;

            // Draw immediately with the current ceiling (may be mid-animation),
            // then let the animation loop refine it.
            drawHistogram(data);
            scheduleCeilingAnimation();
        } finally {
            computing = false;
        }

        // If another update was requested while we were computing, run again
        if (pendingUpdate) {
            pendingUpdate = false;
            updateHistogram();
        }
    }

    /**
     * Computes a y-axis ceiling from histogram data using the 99.5th
     * percentile across all three channels (excluding the clipped-black bin 0
     * and clipped-white bin 255, which are almost always outlier spikes).
     */
    function computeCeiling(histograms: {
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
    }): number {
        const allBins = [
            ...histograms.r.subarray(1, 255),
            ...histograms.g.subarray(1, 255),
            ...histograms.b.subarray(1, 255),
        ].sort((a, b) => a - b);

        const p995 = allBins[Math.floor(allBins.length * 0.995)];
        return Math.max(p995, 1);
    }

    /**
     * 3-tap triangle (1-2-1) smooth to eliminate combing artifacts caused
     * by 8-bit source quantisation through non-linear adjustments.
     */
    function smooth(bins: Uint32Array): Float32Array {
        const out = new Float32Array(256);
        out[0] = bins[0];
        out[255] = bins[255];
        for (let i = 1; i < 255; i++) {
            out[i] = (bins[i - 1] + 2 * bins[i] + bins[i + 1]) / 4;
        }
        return out;
    }

    /**
     * Draws the R, G, B histogram curves onto the 2D canvas using
     * additive blending so overlapping channels produce natural mixes
     * (R+G = yellow, R+B = magenta, etc.)
     */
    function drawHistogram(histograms: {
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
    }) {
        const ctx = canvas.getContext("2d")!;
        const { width, height } = canvas;

        const smoothR = smooth(histograms.r);
        const smoothG = smooth(histograms.g);
        const smoothB = smooth(histograms.b);

        // Use the live animated ceiling for normalization
        const norm = (v: number) => Math.min(v, currentCeiling) / currentCeiling;

        ctx.clearRect(0, 0, width, height);
        ctx.globalCompositeOperation = "lighter"; // additive colour blend

        for (const [bins, color] of [
            [smoothR, "rgb(255,0,0)"],
            [smoothG, "rgb(0,255,0)"],
            [smoothB, "rgb(0,0,255)"],
        ] as const) {
            ctx.beginPath();
            ctx.moveTo(0, height);
            for (let i = 0; i < 256; i++) {
                const x = (i / 255) * width;
                const y = height - norm(bins[i]) * height;
                ctx.lineTo(x, y);
            }
            ctx.lineTo(width, height);
            ctx.closePath();
            ctx.fillStyle = color;
            ctx.fill();
        }
    }
</script>

<canvas bind:this={canvas}></canvas>

<style>
    canvas {
        width: 100%;
        aspect-ratio: 2 / 1;
        border-bottom-left-radius: 4px;
        border-bottom-right-radius: 4px;
        border-top-right-radius: 8px;
        border-top-left-radius: 8px;
        background: var(--bg1);
    }
</style>