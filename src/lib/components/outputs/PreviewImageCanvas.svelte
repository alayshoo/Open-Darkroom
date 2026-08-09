<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import type {
        GPUCanvasLink,
        GPUImage,
        Renderer,
    } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createRenderer } from "$lib/gpu/renderer";
    import { type ImagePayload } from "$lib/types/imagePayload";
    import { type Sliders } from "$lib/types/imgParameters";

    // Props
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
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    let image: GPUImage | null = $state(null);

    let slotWidth = $state(0);
    let slotHeight = $state(0);
    let dpr = $state(1);

    // Track the slot the canvas is centred in, and the device pixel ratio: the
    // fit below is expressed in CSS pixels but has to hold in device pixels.
    $effect(() => {
        const slot = canvas?.parentElement;
        if (!slot) return;

        const observer = new ResizeObserver(([entry]) => {
            slotWidth = entry.contentRect.width;
            slotHeight = entry.contentRect.height;
        });
        observer.observe(slot);

        // A dppx media query only matches the ratio it was built with, so each
        // change needs a fresh query to keep watching.
        let query: MediaQueryList | null = null;
        function trackDpr() {
            query?.removeEventListener("change", trackDpr);
            dpr = window.devicePixelRatio || 1;
            query = window.matchMedia(`(resolution: ${dpr}dppx)`);
            query.addEventListener("change", trackDpr);
        }
        trackDpr();

        return () => {
            observer.disconnect();
            query?.removeEventListener("change", trackDpr);
        };
    });

    // Fit the image to the slot, but never past 1:1 — one image pixel covers at
    // most one device pixel, so the preview is downscaled or shown at native
    // size and never magnified to fill the slot.
    let displaySize = $derived.by(() => {
        if (!imagePayload) return null;

        const nativeWidth = imagePayload.width / dpr;
        const nativeHeight = imagePayload.height / dpr;
        if (!slotWidth || !slotHeight) return { w: nativeWidth, h: nativeHeight };

        const scale = Math.min(
            1,
            slotWidth / nativeWidth,
            slotHeight / nativeHeight,
        );
        return { w: nativeWidth * scale, h: nativeHeight * scale };
    });

    // Link the canvas to GPU and create the renderer once canvas is mounted
    $effect(() => {
        if (!canvas) return;

        let destroyed = false;

        linkCanvas2GPU(canvas, gpu).then((link) => {
            if (destroyed) return;
            canvasLink = link;
            renderer = createRenderer(gpu, canvasLink);
        });

        return () => {
            destroyed = true;
            image?.texture.destroy();
            renderer?.destroy();
            renderer = null;
            canvasLink = null;
            image = null;
        };
    });

    // Load image when src changes
    $effect(() => {
        if (!renderer || !imagePayload) return;

        const localRenderer = renderer;

        // Resize canvas to match image
        canvas.width = imagePayload.width;
        canvas.height = imagePayload.height;

        // Re-link canvas after resize
        linkCanvas2GPU(canvas, gpu).then((link) => {
            canvasLink = link;

            // Clean up old texture
            image?.texture.destroy();

            // Upload via the raw pixel path
            image = uploadRawPixelsToGPU(
                gpu.device,
                imagePayload.pixels,
                imagePayload.width,
                imagePayload.height,
            );

            localRenderer.loadImage(image);
            localRenderer.render();
        });
    });

    // Re-render on adjustment changes
    $effect(() => {
        if (!renderer || !image) return;
        const _ = { ...sliders };
        renderer.setSliders(sliders);
        renderer.render();
    });
</script>

<canvas
    bind:this={canvas}
    style={displaySize
        ? `width: ${displaySize.w}px; height: ${displaySize.h}px;`
        : "width: 0; height: 0;"}
></canvas>

<style>
    canvas {
        display: block;
        max-width: 100%;
        max-height: 100%;
    }
</style>
