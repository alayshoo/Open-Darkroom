<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import type { Adjustments } from "$lib/types/adjustments";
    import { initGPU, type GPUContext } from "$lib/gpu/context";
    import { uploadImageToGPU, type GPUImage } from "$lib/gpu/texture";
    import { createRenderer, type Renderer } from "$lib/gpu/renderer";

    let {
        adjustments,
        imageSrc,
    }: {
        adjustments: Adjustments;
        imageSrc: string | null;
    } = $props();

    let canvas: HTMLCanvasElement;
    let renderer: Renderer | null = $state(null);
    let gpu: GPUContext | null = $state(null);
    let image: GPUImage | null = $state(null);

    // Init WebGPU once canvas is mounted
    $effect(() => {
        if (!canvas) return;

        let destroyed = false;

        initGPU(canvas).then((g) => {
            if (destroyed) return;
            gpu = g;
            renderer = createRenderer(gpu);
        });

        return () => {
            destroyed = true;
            image?.texture.destroy();
            renderer?.destroy();
            renderer = null;
            gpu = null;
            image = null;
        };
    });

    // Load image when src changes
    $effect(() => {
        if (!gpu || !renderer || !imageSrc) return;

        const localGpu = gpu;
        const localRenderer = renderer;

        (async () => {
            const res = await fetch(imageSrc);
            const blob = await res.blob();
            const bitmap = await createImageBitmap(blob);

            // Resize canvas to image
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;

            localGpu.context.configure({
                device: localGpu.device,
                format: localGpu.format,
                usage:
                    GPUTextureUsage.RENDER_ATTACHMENT |
                    GPUTextureUsage.COPY_DST,
            });

            // Clean up old image
            image?.texture.destroy();

            // Upload with sRGB decode for browser images
            image = await uploadImageToGPU(localGpu.device, bitmap);
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
