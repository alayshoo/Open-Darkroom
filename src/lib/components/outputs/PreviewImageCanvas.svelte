<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import type { Snippet } from "svelte";
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
    import { proofing } from "$lib/gpu/proofing.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { cropView, type Crop } from "$lib/types/crop";

    // Props
    let {
        sliders,
        curveLut,
        imagePayload,
        crop,
        overlay,
    }: {
        sliders: Sliders;
        // The baked point curves, or null while every curve is the identity.
        curveLut: Float32Array | null;
        imagePayload: ImagePayload | null;
        crop: Crop;
        // Drawn over the canvas, sized to it.
        overlay?: Snippet;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    let image: GPUImage | null = $state(null);

    // The part of the preview that reaches the canvas, which is sized to it.
    let view = $derived(
        imagePayload ? cropView(crop, imagePayload.width, imagePayload.height) : null,
    );

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
        if (!view) return null;

        const nativeWidth = view.width / dpr;
        const nativeHeight = view.height / dpr;
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

            localRenderer.loadImage(image, imagePayload.srcToWorking);
            // The canvas backing store is the preview texture, so this is how
            // far the render sits from full resolution.
            localRenderer.setView(
                imagePayload.width / imagePayload.fullWidth,
                imagePayload.fullWidth,
                imagePayload.fullHeight,
            );
            localRenderer.requestRender();
        });
    });

    // The crop is applied by the last stage, so a change resizes the canvas
    // without touching the image already on the GPU.
    $effect(() => {
        if (!renderer || !image || !view) return;
        if (canvas.width !== view.width || canvas.height !== view.height) {
            canvas.width = view.width;
            canvas.height = view.height;
            void linkCanvas2GPU(canvas, gpu);
        }
        renderer.setCrop(view);
        renderer.requestRender();
    });

    // The destination the chain encodes for: the display, unless the export
    // space is being proofed through it.
    $effect(() => {
        if (!renderer) return;
        renderer.setOutputSpace(gpu.displaySpace, proofing.space);
        renderer.requestRender();
    });

    $effect(() => {
        if (!renderer) return;
        renderer.setToneMap(colorSpaceDebug.toneMap);
        renderer.requestRender();
    });

    $effect(() => {
        if (!renderer) return;
        renderer.setCurves(curveLut);
        renderer.requestRender();
    });

    // Re-render on adjustment changes. Both calls are bookkeeping — a buffer
    // write and a flag — so a drag never waits on the GPU here.
    $effect(() => {
        if (!renderer || !image) return;
        const _ = { ...sliders };
        renderer.setSliders(sliders);
        renderer.requestRender();
    });
</script>

<div
    class="preview-frame relative"
    style={displaySize
        ? `width: ${displaySize.w}px; height: ${displaySize.h}px;`
        : "width: 0; height: 0;"}
>
    <canvas bind:this={canvas}></canvas>
    {#if displaySize}
        {@render overlay?.()}
    {/if}
</div>

<style>
    .preview-frame {
        max-width: 100%;
        max-height: 100%;
    }

    canvas {
        display: block;
        width: 100%;
        height: 100%;
    }
</style>
