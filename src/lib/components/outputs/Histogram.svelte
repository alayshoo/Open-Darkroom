<!-- src/lib/components/Histogram.svelte -->
<script lang="ts">
    import type { Adjustments } from "$lib/types/adjustments";
    import type { GPUImage, HistogramPipeline } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { uploadImageToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createHistogramPipeline } from "$lib/gpu/pipelines/histogramPipeline";

    let {
        adjustments,
        imageSrc,
    }: {
        adjustments: Adjustments;
        imageSrc: string | null;
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
        };
    });

    // Load image when src changes
    $effect(() => {
        if (!histPipeline || !imageSrc) return;

        let cancelled = false;

        (async () => {
            const res = await fetch(imageSrc);
            const blob = await res.blob();
            const bitmap = await createImageBitmap(blob);

            if (cancelled) { bitmap.close(); return; }

            // Clean up previous image
            image?.texture.destroy();

            image = await uploadImageToGPU(gpu.device, bitmap);
            bitmap.close();

            // Trigger an initial histogram compute
            await updateHistogram();
        })();

        return () => { cancelled = true; };
    });

    // Re-compute histogram when adjustments change
    $effect(() => {
        if (!image || !histPipeline) return;
        // Read adjustments to establish Svelte reactivity tracking
        const _ = { ...adjustments };
        updateHistogram();
    });

    async function updateHistogram() {
        if (!image || !histPipeline) return;

        const data = await histPipeline.computeHistogram(image.texture, adjustments);
        lastHistData = data;
        drawHistogram(data);
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

        // Find the global max across all channels for normalisation
        const max = Math.max(...histograms.r, ...histograms.g, ...histograms.b);
        if (max === 0) return; // avoid division by zero on blank images

        ctx.clearRect(0, 0, width, height);
        ctx.globalCompositeOperation = "lighter"; // additive colour blend

        for (const [bins, color] of [
            [histograms.r, "rgb(255,0,0)"],
            [histograms.g, "rgb(0,255,0)"],
            [histograms.b, "rgb(0,0,255)"],
        ] as const) {
            ctx.beginPath();
            ctx.moveTo(0, height);
            for (let i = 0; i < 256; i++) {
                const x = (i / 255) * width;
                const y = height - (bins[i] / max) * height;
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
