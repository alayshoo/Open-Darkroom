<!-- src/lib/components/Histogram.svelte -->
<script lang="ts">
    import { untrack } from "svelte";
    import { type Sliders } from "$lib/types/imgParameters";
    import type { GPUCanvasLink, GPUImage, HistogramPipeline } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createHistogramPipeline } from "$lib/gpu/pipelines/histogramPipeline";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
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
    let canvasLink: GPUCanvasLink | null = $state(null);
    let histPipeline: HistogramPipeline | null = $state(null);
    let image: GPUImage | null = $state(null);

    // The bins are binned, normalised and drawn without ever reaching the CPU,
    // so a redraw needs nothing kept on this side — not the last data, and not
    // a canvas context to paint it with.
    let dirty = false;
    let frameHandle: number | null = null;

    function requestRender() {
        dirty = true;
        if (frameHandle === null) frameHandle = requestAnimationFrame(onFrame);
    }

    function onFrame() {
        frameHandle = null;
        if (!dirty) return;
        // A request made before the canvas or the image was ready outlives the
        // frame that could not serve it.
        if (draw()) dirty = false;
    }

    function draw(): boolean {
        if (!histPipeline || !image || !canvasLink) return false;
        if (!debugStats.histogramEnabled) return false;
        // Nothing to render into until the observer has sized the backing store.
        if (canvas.width === 0 || canvas.height === 0) return false;

        histPipeline.render(
            image.texture,
            sliders,
            canvasLink.canvasConfig.getCurrentTexture().createView(),
        );
        return true;
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
                    // Resizing drops the swapchain contents, so the curves have
                    // to be drawn again — but only drawn, not recomputed.
                    requestRender();
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

    // Link the canvas and create the pipeline once on mount
    $effect(() => {
        if (!canvas) return;

        let destroyed = false;
        histPipeline = createHistogramPipeline(gpu);

        // Premultiplied so the panel's own background shows through wherever
        // the curves do not reach, as it did behind the 2D canvas.
        linkCanvas2GPU(canvas, gpu, "premultiplied").then((link) => {
            if (destroyed) return;
            canvasLink = link;
            requestRender();
        });

        return () => {
            destroyed = true;
            image?.texture.destroy();
            histPipeline?.destroy();
            histPipeline = null;
            canvasLink = null;
            image = null;
            if (frameHandle !== null) {
                cancelAnimationFrame(frameHandle);
                frameHandle = null;
            }
            dirty = false;
        };
    });

    // Load image when payload changes
    $effect(() => {
        if (!histPipeline || !imagePayload) return;

        // Clean up previous image (untracked to avoid re-triggering this effect)
        untrack(() => image?.texture.destroy());

        // Upload via the raw pixel path (same as PreviewImageCanvas)
        image = uploadRawPixelsToGPU(
            gpu.device,
            imagePayload.pixels,
            imagePayload.width,
            imagePayload.height,
        );
        requestRender();
    });

    // Re-render when adjustments change. The debug flag is read here rather than
    // only inside the draw so that switching it back on refreshes immediately
    // instead of waiting for the next slider move.
    $effect(() => {
        if (!image || !histPipeline) return;
        // Read adjustments to establish Svelte reactivity tracking
        const _ = { ...sliders };
        if (!debugStats.histogramEnabled) return;
        requestRender();
    });
</script>

<canvas
    class="w-full aspect-[2/1] rounded-[7px]"
    bind:this={canvas}
></canvas>

<style>
    canvas {
        background: var(--bg1);
    }
</style>
