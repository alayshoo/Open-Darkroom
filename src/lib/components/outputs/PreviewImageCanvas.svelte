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
    import {
        cropView,
        rotatedBounds,
        rotationCanvas,
        uprightFrame,
        type Crop,
    } from "$lib/types/crop";

    // Props
    let {
        sliders,
        curveLut,
        imagePayload,
        crop,
        showWhole = false,
        overlay,
    }: {
        sliders: Sliders;
        // The baked point curves, or null while every curve is the identity.
        curveLut: Float32Array | null;
        imagePayload: ImagePayload | null;
        crop: Crop;
        // Show all of the turned and rotated image the crop is cut from, laid
        // out on its upright frame, rather than the crop itself.
        showWhole?: boolean;
        // Drawn over the frame, sized to it.
        overlay?: Snippet;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let frame: HTMLDivElement;
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    let image = $state<GPUImage | null>(null);

    // The part of the preview that reaches the canvas, which is sized to it.
    // Taken from the texture on the GPU, so it never runs ahead of the upload.
    //
    // Showing the whole image, the canvas is one size at every angle, since a
    // resize clears it and a rotation drag would flash on every step. The
    // frame is the upright image, and what has to fit the slot is the rotated
    // image's own bounds.
    const viewOf = (c: Crop) => (image ? cropView(c, image.width, image.height) : null);
    let view = $derived(
        image && showWhole ? viewOf(rotationCanvas(crop, image.width, image.height)) : viewOf(crop),
    );
    let frameView = $derived(showWhole ? viewOf(uprightFrame(crop)) : view);
    let fitView = $derived(
        image && showWhole ? viewOf(rotatedBounds(crop, image.width, image.height)) : view,
    );

    // The canvas against the frame, centred on it, in percent of the frame.
    let canvasStyle = $derived.by(() => {
        if (!view || !frameView) return "";
        const w = (view.width / frameView.width) * 100;
        const h = (view.height / frameView.height) * 100;
        return `width: ${w}%; height: ${h}%; left: ${(100 - w) / 2}%; top: ${(100 - h) / 2}%;`;
    });

    let slotWidth = $state(0);
    let slotHeight = $state(0);
    let dpr = $state(1);

    // Track the slot the canvas is centred in, and the device pixel ratio: the
    // fit below is expressed in CSS pixels but has to hold in device pixels.
    $effect(() => {
        // The frame's parent, not the frame: the frame is sized from this
        // measurement, so observing it would feed each fit back into the next.
        const slot = frame?.parentElement;
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

    // Fit the image to the slot, never past 1:1 (one image pixel per device
    // pixel), then draw the frame at the same scale.
    let displaySize = $derived.by(() => {
        if (!fitView || !frameView) return null;

        const frameWidth = frameView.width / dpr;
        const frameHeight = frameView.height / dpr;
        if (!slotWidth || !slotHeight) return { w: frameWidth, h: frameHeight };

        const scale = Math.min(
            1,
            slotWidth / (fitView.width / dpr),
            slotHeight / (fitView.height / dpr),
        );
        return { w: frameWidth * scale, h: frameHeight * scale };
    });

    // Link the canvas to GPU and create the renderer once canvas is mounted
    $effect(() => {
        if (!canvas) return;

        let destroyed = false;

        linkCanvas2GPU(canvas, gpu, "premultiplied").then((link) => {
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

        linkCanvas2GPU(canvas, gpu, "premultiplied").then((link) => {
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
            void linkCanvas2GPU(canvas, gpu, "premultiplied");
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
    bind:this={frame}
    class="preview-frame relative"
    style={displaySize
        ? `width: ${displaySize.w}px; height: ${displaySize.h}px;`
        : "width: 0; height: 0;"}
>
    <canvas bind:this={canvas} style={canvasStyle}></canvas>
    {#if displaySize}
        {@render overlay?.()}
    {/if}
</div>

<style>
    .preview-frame {
        max-width: 100%;
        max-height: 100%;
    }

    /* Premultiplied, so wherever the render leaves the image the backdrop
       shows through. */
    canvas {
        position: absolute;
        display: block;
    }
</style>
