<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import type { Adjustments } from "$lib/types/adjustments";
    import type { GPUImage } from "$lib/gpu/texture";
    import { initGPU } from "$lib/gpu/context";
    import { createRenderer, type Renderer } from "$lib/gpu/renderer";

    let {
        adjustments,
        imageData,
    }: {
        adjustments: Adjustments;
        imageData: GPUImage | null;
    } = $props();

    let canvas: HTMLCanvasElement;
    let renderer: Renderer | null = null;

    // Init WebGPU + renderer once canvas is mounted
    $effect(() => {
        if (!canvas) return;

        let destroyed = false;
        let localRenderer: Renderer | null = null;

        initGPU(canvas).then((gpu) => {
            if (destroyed) return;
            localRenderer = createRenderer(gpu);
            renderer = localRenderer;
        });

        return () => {
            destroyed = true;
            localRenderer?.destroy();
            renderer = null;
        };
    });

    // Load image when it changes
    $effect(() => {
        if (renderer && imageData) {
            renderer.loadImage(imageData);
            renderer.render();
        }
    });

    // Re-render on adjustment changes
    $effect(() => {
        if (!renderer) return;
        renderer.setAdjustments(adjustments);
        renderer.render();
    });
</script>

<canvas bind:this={canvas}></canvas>
