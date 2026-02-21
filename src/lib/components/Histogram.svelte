<!--<script lang="ts">
    import { onMount } from "svelte";
    import { draw } from "svelte/transition";
    import type { Adjustments } from "$lib/types/adjustments";
    import { initGPU, type GPUContext } from "$lib/gpu/context";
    import { uploadImageToGPU, type GPUImage } from "$lib/gpu/gpuTextureUpload";
    import { createRenderer, type Renderer } from "$lib/gpu/renderer";

    let { imageSrc }: { imageSrc: string | null; } = $props();
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


    function drawHistogram(histograms: {
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
    }) {
        const ctx = canvas.getContext("2d")!;
        const { width, height } = canvas;

        const max = Math.max(...histograms.r, ...histograms.g, ...histograms.b);

        ctx.clearRect(0, 0, width, height);
        ctx.globalCompositeOperation = "lighter";               // sets additive color blend mode

        for (const [bins, color] of [
            [histograms.r, "rgb(255,0,0)"],
            [histograms.g, "rgb(0,255,0)"],
            [histograms.b, "rgb(0,0,255)"],
        ] as const) {                                           // for each pair of histogram data and histogram draw color
            ctx.beginPath();                                    // create a path
            ctx.moveTo(0, height);                              // move to bottom left corner
            for (let i = 0; i < 256; i++) {                     // for each point in histogram x axis
                const x = (i / 255) * width;
                const y = height - (bins[i] / max) * height;    // get histogram value to plot
                ctx.lineTo(x, y);                               // draw line to that point
            }
            ctx.lineTo(width, height);
            ctx.closePath();                                    // close path and apply fill colour
            ctx.fillStyle = color;
            ctx.fill();
        }
    }

    $effect(() => {
        if (imageSrc && canvas) {
            drawHistogram();
        }
    });
</script>

<canvas bind:this={canvas} width={256} height={128}></canvas> -->
