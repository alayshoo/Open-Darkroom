<!-- src/lib/components/outputs/ColorSpacePanel.svelte -->
<script lang="ts">
    import { untrack } from "svelte";
    import { type Sliders } from "$lib/types/imgParameters";
    import type { GPUCanvasLink, GPUImage } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import {
        createColorSpacePipeline,
        type ColorSpacePipeline,
        type GamutStats,
    } from "$lib/gpu/pipelines/colorSpacePipeline";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { GAMUT_ORDER, GAMUT_TINTS } from "$lib/gpu/gamutOverlay";
    import { OUTPUT_SPACE_LABELS, WORKING_SPACE } from "$lib/gpu/outputSpace";
    import { type ImagePayload } from "$lib/types/imagePayload";

    let {
        sliders,
        imagePayload,
    }: {
        sliders: Sliders;
        imagePayload: ImagePayload | null;
    } = $props();

    const gpu = getGPU()!;

    let chromaticityCanvas: HTMLCanvasElement;
    let volumeCanvas: HTMLCanvasElement;
    let chromaticityLink: GPUCanvasLink | null = $state(null);
    let volumeLink: GPUCanvasLink | null = $state(null);
    let pipeline: ColorSpacePipeline | null = $state(null);
    let image: GPUImage | null = $state(null);
    let stats: GamutStats | null = $state(null);

    // The plots are binned, normalised and drawn without reaching the CPU, so a
    // redraw needs nothing kept on this side. The tallies are the exception —
    // percentages only mean anything as numbers — so they are read back once a
    // drag has settled rather than on every frame of it.
    const STATS_SETTLE_MS = 160;

    let dirty = false;
    let frameHandle: number | null = null;
    let statsHandle: ReturnType<typeof setTimeout> | null = null;

    function requestRender() {
        dirty = true;
        if (frameHandle === null) frameHandle = requestAnimationFrame(onFrame);
    }

    function onFrame() {
        frameHandle = null;
        if (!dirty) return;
        // A request made before the canvases or the image were ready outlives
        // the frame that could not serve it.
        if (draw()) dirty = false;
    }

    function draw(): boolean {
        if (!pipeline || !image || !chromaticityLink || !volumeLink) return false;
        if (chromaticityCanvas.width === 0 || volumeCanvas.width === 0) return false;

        pipeline.render(image.texture, sliders, {
            chromaticity: chromaticityLink.canvasConfig.getCurrentTexture().createView(),
            volume: volumeLink.canvasConfig.getCurrentTexture().createView(),
        });

        if (statsHandle !== null) clearTimeout(statsHandle);
        statsHandle = setTimeout(readStats, STATS_SETTLE_MS);
        return true;
    }

    async function readStats() {
        statsHandle = null;
        const current = pipeline;
        if (!current) return;
        const read = await current.readStats();
        // The panel may have been closed while the map was in flight.
        if (pipeline === current) stats = read;
    }

    /** Keep a canvas' backing store at native device pixels. */
    function observe(canvas: HTMLCanvasElement) {
        const ro = new ResizeObserver((entries) => {
            for (const entry of entries) {
                let w: number, h: number;
                if (entry.devicePixelContentBoxSize) {
                    w = entry.devicePixelContentBoxSize[0].inlineSize;
                    h = entry.devicePixelContentBoxSize[0].blockSize;
                } else {
                    const dpr = window.devicePixelRatio || 1;
                    w = Math.round(entry.contentBoxSize[0].inlineSize * dpr);
                    h = Math.round(entry.contentBoxSize[0].blockSize * dpr);
                }

                if (canvas.width !== w || canvas.height !== h) {
                    canvas.width = w;
                    canvas.height = h;
                    // Resizing drops the swapchain contents, so the plots have
                    // to be drawn again — but only drawn, not recomputed.
                    requestRender();
                }
            }
        });

        try {
            ro.observe(canvas, { box: "device-pixel-content-box" });
        } catch {
            ro.observe(canvas, { box: "content-box" });
        }
        return ro;
    }

    $effect(() => {
        if (!chromaticityCanvas || !volumeCanvas) return;
        const observers = [observe(chromaticityCanvas), observe(volumeCanvas)];
        return () => observers.forEach((ro) => ro.disconnect());
    });

    // Link the canvases and create the pipeline once on mount. sRGB on both:
    // the plots are interface, drawn to sit beside the CSS around them.
    $effect(() => {
        if (!chromaticityCanvas || !volumeCanvas) return;

        let destroyed = false;
        pipeline = createColorSpacePipeline(gpu);

        void Promise.all([
            linkCanvas2GPU(chromaticityCanvas, gpu, "premultiplied", "srgb"),
            linkCanvas2GPU(volumeCanvas, gpu, "premultiplied", "srgb"),
        ]).then(([chroma, vol]) => {
            if (destroyed) return;
            chromaticityLink = chroma;
            volumeLink = vol;
            requestRender();
        });

        return () => {
            destroyed = true;
            image?.texture.destroy();
            pipeline?.destroy();
            pipeline = null;
            chromaticityLink = null;
            volumeLink = null;
            image = null;
            stats = null;
            if (frameHandle !== null) {
                cancelAnimationFrame(frameHandle);
                frameHandle = null;
            }
            if (statsHandle !== null) {
                clearTimeout(statsHandle);
                statsHandle = null;
            }
            dirty = false;
        };
    });

    // The overview, not the preview: the gamut of the image is the gamut of the
    // whole frame regardless of what the viewport is showing.
    $effect(() => {
        if (!pipeline || !imagePayload) return;

        untrack(() => image?.texture.destroy());

        image = uploadRawPixelsToGPU(
            gpu.device,
            imagePayload.overviewPixels,
            imagePayload.overviewWidth,
            imagePayload.overviewHeight,
        );
        pipeline.setSourceSpace(imagePayload.srcToWorking);
        requestRender();
    });

    $effect(() => {
        if (!pipeline) return;
        pipeline.setOverlays(colorSpaceDebug.overlays);
        // A space that has just been switched off leaves a stale tally behind,
        // and the next pass is what clears it.
        stats = null;
        requestRender();
    });

    $effect(() => {
        if (!image || !pipeline) return;
        // Read the adjustments to establish Svelte's reactivity tracking.
        const _ = { ...sliders };
        requestRender();
    });

    function percentage(id: (typeof GAMUT_ORDER)[number]) {
        if (!stats || !colorSpaceDebug.overlays[id]) return null;
        if (stats.counted === 0) return null;
        return (100 * (stats.outside[id] ?? 0)) / stats.counted;
    }

    function formatPercentage(value: number) {
        if (value === 0) return "0%";
        if (value < 0.1) return "<0.1%";
        return `${value.toFixed(1)}%`;
    }
</script>

<div class="colour-space flex flex-col gap-3">
    <div class="panel glass rounded-[12px] p-2.5">
        <h2>Chromaticity · CIE 1931</h2>
        <canvas class="chromaticity" bind:this={chromaticityCanvas}></canvas>
        <p class="axes"><span>x</span><span>y</span></p>

        <ul class="spaces">
            {#each GAMUT_ORDER as id (id)}
                {@const outside = percentage(id)}
                <li>
                    <label>
                        <input
                            type="checkbox"
                            checked={colorSpaceDebug.overlays[id]}
                            onchange={() => colorSpaceDebug.toggleOverlay(id)}
                        />
                        <span class="swatch" style="background: {GAMUT_TINTS[id]}"></span>
                        <span class="name">{OUTPUT_SPACE_LABELS[id]}</span>
                        <span class="outside">
                            {#if id === WORKING_SPACE}
                                working
                            {:else if outside !== null}
                                {formatPercentage(outside)} out
                            {:else if colorSpaceDebug.overlays[id]}
                                …
                            {/if}
                        </span>
                    </label>
                </li>
            {/each}
        </ul>
        <p class="note">
            Measured in the working space, after the adjustments and before the
            output transform.
        </p>
    </div>

    <div class="panel glass rounded-[12px] p-2.5">
        <h2>Colour volume · hue × light</h2>
        <canvas class="volume" bind:this={volumeCanvas}></canvas>
        <p class="axes"><span>hue</span><span>lightness ↑</span></p>
    </div>
</div>

<style>
    .colour-space {
        font-family: ui-monospace, "Cascadia Mono", "Consolas", monospace;
        font-size: 10.5px;
        line-height: 1.45;
        color: rgba(255, 255, 255, 0.78);
    }

    .panel {
        box-shadow:
            6px 10px 22px -2px rgba(0, 0, 0, 0.47),
            2px 3px 8px -1px rgba(0, 0, 0, 0.34),
            inset 16px 21px 50px -38px rgba(255, 255, 255, 0.16);
    }

    h2 {
        margin: 0 0 0.4rem;
        font-size: 10.5px;
        font-weight: 600;
        letter-spacing: 0.04em;
        text-transform: uppercase;
        color: rgba(255, 255, 255, 0.5);
    }

    canvas {
        display: block;
        width: 100%;
        border-radius: 6px;
        background: var(--bg1);
    }

    /* The two axes carry the same scale, so the box has to be 0.8 : 0.9 — see
       XY_X_MAX and XY_Y_MAX in chunks/gamutGrids.wgsl. */
    canvas.chromaticity {
        aspect-ratio: 8 / 9;
    }

    canvas.volume {
        aspect-ratio: 2 / 1;
    }

    .axes {
        display: flex;
        justify-content: space-between;
        margin: 0.3rem 0 0;
        color: rgba(255, 255, 255, 0.4);
    }

    .spaces {
        margin: 0.55rem 0 0;
        padding: 0.45rem 0 0;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        list-style: none;
    }

    label {
        display: flex;
        align-items: center;
        gap: 0.45rem;
        padding: 0.12rem 0;
        cursor: pointer;
    }

    input {
        width: 11px;
        height: 11px;
        margin: 0;
        accent-color: rgba(255, 255, 255, 0.72);
        cursor: pointer;
    }

    .swatch {
        width: 9px;
        height: 9px;
        border-radius: 2px;
    }

    .name {
        color: rgba(255, 255, 255, 0.78);
    }

    .outside {
        margin-left: auto;
        font-variant-numeric: tabular-nums;
        color: rgba(255, 255, 255, 0.45);
    }

    .note {
        margin: 0.5rem 0 0;
        padding-top: 0.4rem;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        color: rgba(255, 255, 255, 0.38);
        line-height: 1.4;
    }
</style>
