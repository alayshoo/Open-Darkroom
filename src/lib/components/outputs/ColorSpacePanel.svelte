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
    import { proofing } from "$lib/gpu/proofing.svelte";
    import { GAMUT_ORDER, GAMUT_TINTS } from "$lib/gpu/gamutOverlay";
    import { OUTPUT_SPACE_LABELS, WORKING_SPACE } from "$lib/gpu/outputSpace";
    import { type ImagePayload } from "$lib/types/imagePayload";
    import type { RolloffMode } from "$lib/types/toneMap";
    import PreviewDebugCard from "./PreviewDebugCard.svelte";

    let {
        sliders,
        curveLut,
        imagePayload,
    }: {
        sliders: Sliders;
        // The baked point curves, or null while every curve is the identity.
        curveLut: Float32Array | null;
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

    // The out-of-gamut tallies are read back once a drag has settled.
    const STATS_SETTLE_MS = 160;

    let dirty = false;
    let frameHandle: number | null = null;
    let statsHandle: ReturnType<typeof setTimeout> | null = null;
    let statsReading = false;
    let volumeAspect = $state(1);

    function requestRender() {
        dirty = true;
        if (frameHandle === null) frameHandle = requestAnimationFrame(onFrame);
    }

    function onFrame() {
        frameHandle = null;
        // A request made before the canvases or the image were ready outlives
        // the frame that could not serve it.
        if (dirty) {
            if (draw()) {
                dirty = false;
                viewDirty = false;
            }
            return;
        }
        if (viewDirty && redraw()) viewDirty = false;
    }

    /** The views to draw into, or null while nothing is ready to receive one. */
    function targets() {
        if (!chromaticityLink || chromaticityCanvas.width === 0) return null;

        const showVolume =
            colorSpaceDebug.volume && volumeLink !== null && volumeCanvas.width > 0;

        return {
            chromaticity: chromaticityLink.canvasConfig.getCurrentTexture().createView(),
            volume: showVolume
                ? volumeLink!.canvasConfig.getCurrentTexture().createView()
                : undefined,
        };
    }

    function draw(): boolean {
        if (!pipeline || !image) return false;
        const views = targets();
        if (!views) return false;

        pipeline.render(image.texture, sliders, views);

        if (statsHandle !== null) clearTimeout(statsHandle);
        statsHandle = setTimeout(readStats, STATS_SETTLE_MS);
        return true;
    }

    // Turning the box changes where it is seen from, not what was measured, so
    // it costs a draw rather than another pass over the image.
    let viewDirty = false;

    function requestRedraw() {
        viewDirty = true;
        if (frameHandle === null) frameHandle = requestAnimationFrame(onFrame);
    }

    function redraw(): boolean {
        if (!pipeline) return false;
        const views = targets();
        if (!views) return false;
        pipeline.redraw(views);
        return true;
    }

    async function readStats() {
        statsHandle = null;
        const current = pipeline;
        if (!current) return;
        // The staging buffer can only be mapped once at a time.
        if (statsReading) {
            statsHandle = setTimeout(readStats, STATS_SETTLE_MS);
            return;
        }
        statsReading = true;
        try {
            const read = await current.readStats();
            if (pipeline === current) stats = read;
        } catch {
            // The pipeline was destroyed while the map was in flight.
        } finally {
            statsReading = false;
        }
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
                    if (canvas === volumeCanvas && h > 0) volumeAspect = w / h;
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
            viewDirty = false;
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
        pipeline.setOverlays({
            enabled: colorSpaceDebug.overlays,
            transformed: colorSpaceDebug.transformed,
            volume: colorSpaceDebug.volume,
            destination: gpu.displaySpace,
        });
        // The current tallies no longer match what is being measured.
        stats = null;
        requestRender();
    });

    // The transform the panel can measure through is the one the preview is
    // rendered with, soft proofing included.
    $effect(() => {
        if (!pipeline) return;
        pipeline.setOutputSpace(gpu.displaySpace, proofing.space);
        requestRender();
    });

    $effect(() => {
        if (!pipeline) return;
        pipeline.setToneMap(colorSpaceDebug.toneMap);
        requestRender();
    });

    $effect(() => {
        if (!pipeline) return;
        pipeline.setCurves(curveLut);
        requestRender();
    });

    $effect(() => {
        if (!pipeline) return;
        pipeline.setVolumeView({
            yaw: colorSpaceDebug.yaw,
            pitch: colorSpaceDebug.pitch,
            aspect: volumeAspect,
        });
        requestRedraw();
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

    const ROLLOFF_MODES: { id: RolloffMode; label: string }[] = [
        { id: "luminance", label: "Luminance" },
        { id: "perChannel", label: "Per channel" },
    ];

    function formatPercentage(value: number) {
        if (value === 0) return "0%";
        if (value < 0.1) return "<0.1%";
        return `${value.toFixed(1)}%`;
    }

    // Dragging turns the box. The pointer is captured so a drag that leaves the
    // canvas keeps turning it rather than stopping at the edge.
    let turning = false;
    let last = { x: 0, y: 0 };

    const TURN_PER_PIXEL = 0.011;

    function startTurn(event: PointerEvent) {
        turning = true;
        last = { x: event.clientX, y: event.clientY };
        volumeCanvas.setPointerCapture(event.pointerId);
    }

    function turn(event: PointerEvent) {
        if (!turning) return;
        colorSpaceDebug.turn(
            (event.clientX - last.x) * TURN_PER_PIXEL,
            (event.clientY - last.y) * TURN_PER_PIXEL,
        );
        last = { x: event.clientX, y: event.clientY };
    }

    function endTurn(event: PointerEvent) {
        turning = false;
        volumeCanvas.releasePointerCapture(event.pointerId);
    }
</script>

<div class="colour-space flex flex-col gap-3">
    <div class="panel glass rounded-[12px] p-2.5">
        <h2>Highlight roll-off</h2>
        <label class="rolloff">
            <span class="name">Strength</span>
            <input
                class="range"
                type="range"
                min="0"
                max="100"
                step="1"
                bind:value={colorSpaceDebug.rolloff}
                ondblclick={() => (colorSpaceDebug.rolloff = 0)}
            />
            <span class="outside">{colorSpaceDebug.rolloff}</span>
        </label>
        {#each ROLLOFF_MODES as mode (mode.id)}
            <label>
                <input
                    type="radio"
                    name="rolloff-mode"
                    checked={colorSpaceDebug.rolloffMode === mode.id}
                    onchange={() => (colorSpaceDebug.rolloffMode = mode.id)}
                />
                <span class="name">{mode.label}</span>
            </label>
        {/each}

        <p class="note">
            {#if colorSpaceDebug.rolloffMode === "luminance"}
                Compresses brightness and keeps each hue, which fades to white
                as it nears the top.
            {:else}
                Compresses R, G and B separately, like film: bright colours
                desaturate sooner but their hues can drift.
            {/if}
            Applied to the preview, histogram and export, and kept when this
            panel is closed.
        </p>
    </div>

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
                            {#if id === WORKING_SPACE && !colorSpaceDebug.transformed}
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

        <label class="measure">
            <input
                type="checkbox"
                checked={colorSpaceDebug.transformed}
                onchange={() => (colorSpaceDebug.transformed = !colorSpaceDebug.transformed)}
            />
            <span class="name">After output transform</span>
        </label>

        <p class="note">
            {#if colorSpaceDebug.transformed}
                What the preview is showing, fitted to
                {OUTPUT_SPACE_LABELS[gpu.displaySpace]}{proofing.space
                    ? ` and proofed against ${OUTPUT_SPACE_LABELS[proofing.space]}`
                    : ""}. Nothing can fall outside the space being rendered to,
                so a zero here says the compression worked.
            {:else}
                The colour the adjustments are asking for, measured in the
                working space before the output transform.
            {/if}
        </p>
    </div>

    <div class="panel glass rounded-[12px] p-2.5">
        <div class="volume-head">
            <h2>Colour volume · Oklab</h2>
            <label class="switch">
                <input
                    type="checkbox"
                    checked={colorSpaceDebug.volume}
                    onchange={() => (colorSpaceDebug.volume = !colorSpaceDebug.volume)}
                />
            </label>
        </div>

        <!-- Kept mounted while off so the panel keeps its size. -->
        <div class="volume-slot" class:off={!colorSpaceDebug.volume}>
            <canvas
                class="volume"
                bind:this={volumeCanvas}
                onpointerdown={startTurn}
                onpointermove={turn}
                onpointerup={endTurn}
                onpointercancel={endTurn}
            ></canvas>
            {#if !colorSpaceDebug.volume}
                <p class="placeholder">off</p>
            {/if}
        </div>

        <p class="axes">
            <span>L ↑ · a/b</span>
            <span>{colorSpaceDebug.volume ? "drag to turn" : ""}</span>
        </p>
    </div>

    <PreviewDebugCard />
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

    /* Square, so the box is a box and turning it never runs a corner off the
       edge of the panel. */
    canvas.volume {
        aspect-ratio: 1 / 1;
        cursor: grab;
        touch-action: none;
    }

    canvas.volume:active {
        cursor: grabbing;
    }

    /* The row carries the heading's margin so the title centres on the switch. */
    .volume-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
        margin-bottom: 0.4rem;
    }

    .volume-head h2 {
        margin: 0;
    }

    .switch {
        padding: 0;
    }

    .volume-slot {
        position: relative;
    }

    .volume-slot.off canvas {
        opacity: 0.25;
        cursor: default;
        pointer-events: none;
    }

    .placeholder {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        margin: 0;
        color: rgba(255, 255, 255, 0.35);
    }

    .measure {
        margin-top: 0.35rem;
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

    input.range {
        flex: 1;
        width: auto;
        min-width: 0;
        height: 11px;
    }

    .rolloff .outside {
        min-width: 2.2em;
        text-align: right;
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
