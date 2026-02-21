<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import type { Adjustments } from "$lib/types/adjustments";
    import type { GPUCanvasLink, GPUImage, Renderer } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { uploadImageToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createRenderer } from "$lib/gpu/renderer";

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
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    let image: GPUImage | null = $state(null);

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
        if (!renderer || !imageSrc) return;

        const localRenderer = renderer;

        (async () => {
            const res = await fetch(imageSrc);
            const blob = await res.blob();
            const bitmap = await createImageBitmap(blob);

            // Resize canvas to image
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;

            // Re-link canvas after resize (reconfigures the GPU context)
            canvasLink = await linkCanvas2GPU(canvas, gpu);

            // Clean up old image
            image?.texture.destroy();

            // Upload image
            image = await uploadImageToGPU(gpu.device, bitmap);
            bitmap.close();

            localRenderer.loadImage(image);
            localRenderer.render();
        })();
    });

    // Re-render on adjustment changes
    $effect(() => {
        if (!renderer || !image) return;
        const _ = { ...adjustments };
        renderer.setAdjustments(adjustments);
        renderer.render();
    });
</script>

<canvas bind:this={canvas}></canvas>

<style>
    canvas {
        max-width: 100%;
        max-height: 100%;
        object-fit: contain;
    }
</style>
