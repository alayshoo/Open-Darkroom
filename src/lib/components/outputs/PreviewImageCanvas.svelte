<!-- src/lib/components/PreviewImageCanvas.svelte -->
<script lang="ts">
    import { untrack, type Snippet } from "svelte";
    import type {
        GPUCanvasLink,
        GPUImage,
        Renderer,
    } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createRenderer } from "$lib/gpu/renderer";
    import {
        REGION_MARGIN,
        createTileSource,
        levelRegion,
        type DetailSource,
        type TileSource,
    } from "$lib/gpu/tileSource";
    import { sharpnessIsVisible } from "$lib/gpu/sharpnessVisibility";
    import { fetchPyramidTiles } from "$lib/utils/openImage";
    import { type ImagePayload } from "$lib/types/imagePayload";
    import { type Sliders } from "$lib/types/imgParameters";
    import { proofing } from "$lib/gpu/proofing.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { type Crop } from "$lib/types/crop";
    import {
        MAX_ZOOM,
        apronFor,
        contains,
        cropLayout,
        levelFor,
        planFrame,
        visibleRegion,
        wholeLayout,
        zoomAbout,
        type Point,
        type SourceMapping,
    } from "$lib/utils/viewport";

    // Props
    let {
        sliders,
        curveLut,
        imagePayload,
        crop,
        showWhole = false,
        zoom = $bindable(null),
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
        // Canvas pixels per image pixel, or null to fit the image to its slot.
        zoom?: number | null;
        // Drawn over the frame, sized to it.
        overlay?: Snippet;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let frame: HTMLDivElement;
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    // The pyramid's top level, which covers the whole image and is always to
    // hand, and whatever finer region of it has been streamed in since.
    let base = $state.raw<GPUImage | null>(null);
    let detail = $state.raw<DetailSource | null>(null);
    let tiles: TileSource | null = null;
    // The texture the renderer is currently reading, so it is only rebound
    // when that changes.
    let bound: GPUImage | null = null;
    // The output point held over the middle of the slot while zoomed.
    let center = $state<Point | null>(null);

    // The canvas covers the whole window, so a zoomed image runs under the
    // panels; the slot is only where the image is centred and fitted.
    let dpr = $state(1);
    let canvasWidth = $state(1);
    let canvasHeight = $state(1);
    let slot = $state.raw<{ center: Point; width: number; height: number } | null>(null);

    $effect(() => {
        // The frame's parent, not the frame: the frame is sized from this
        // measurement, so observing it would feed each fit back into the next.
        const slotElement = frame?.parentElement;
        if (!slotElement) return;

        function measure() {
            dpr = window.devicePixelRatio || 1;
            canvasWidth = Math.max(1, Math.round(window.innerWidth * dpr));
            canvasHeight = Math.max(1, Math.round(window.innerHeight * dpr));
            const r = slotElement!.getBoundingClientRect();
            slot = {
                center: [(r.left + r.width / 2) * dpr, (r.top + r.height / 2) * dpr],
                width: r.width * dpr,
                height: r.height * dpr,
            };
        }

        const observer = new ResizeObserver(measure);
        observer.observe(slotElement);
        window.addEventListener("resize", measure);

        // A dppx media query only matches the ratio it was built with, so each
        // change needs a fresh query to keep watching.
        let query: MediaQueryList | null = null;
        function trackDpr() {
            query?.removeEventListener("change", trackDpr);
            measure();
            query = window.matchMedia(`(resolution: ${dpr}dppx)`);
            query.addEventListener("change", trackDpr);
        }
        trackDpr();

        return () => {
            observer.disconnect();
            window.removeEventListener("resize", measure);
            query?.removeEventListener("change", trackDpr);
        };
    });

    let layout = $derived.by(() => {
        if (!imagePayload || !slot) return null;
        const { fullWidth, fullHeight } = imagePayload;
        return showWhole
            ? wholeLayout(crop, fullWidth, fullHeight, slot)
            : cropLayout(crop, fullWidth, fullHeight, slot, zoom, center);
    });

    // Past 1:1 the chain stays at full resolution, so that is the most detail
    // the sharpening ever has to resolve.
    let renderScale = $derived(Math.min(layout?.placement.scale ?? 1, 1));
    let sharpening = $derived(sharpnessIsVisible(sliders, renderScale));
    let panning = $derived(zoom !== null && !showWhole);

    $effect(() => {
        if (zoom === null) center = null;
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
            tiles?.destroy();
            tiles = null;
            base?.texture.destroy();
            renderer?.destroy();
            renderer = null;
            canvasLink = null;
            base = null;
            detail = null;
        };
    });

    // A new image: its top level goes up whole, and finer tiles stream from
    // its pyramid as the view asks for them.
    $effect(() => {
        if (!renderer || !imagePayload) return;
        const payload = imagePayload;

        tiles?.destroy();
        untrack(() => base)?.texture.destroy();
        detail = null;

        const top = uploadRawPixelsToGPU(gpu.device, payload.pixels, payload.width, payload.height);
        renderer.loadImage(top, payload.srcToWorking);
        base = top;
        bound = top;

        const pyramid = payload.pyramid;
        tiles = pyramid
            ? createTileSource(
                  gpu.device,
                  pyramid,
                  (level, wanted) => fetchPyramidTiles(pyramid.id, level, wanted),
                  (d) => (detail = d),
              )
            : null;
    });

    // Lays the frame out: which level to read, which part of the screen the
    // chain covers, and where its pixels land.
    $effect(() => {
        if (!renderer || !base || !layout || !imagePayload) return;
        const { fullWidth, fullHeight, pyramid } = imagePayload;

        if (canvas.width !== canvasWidth || canvas.height !== canvasHeight) {
            canvas.width = canvasWidth;
            canvas.height = canvasHeight;
            void linkCanvas2GPU(canvas, gpu, "premultiplied");
        }

        const scale = layout.placement.scale;
        const visible = visibleRegion(layout, canvasWidth, canvasHeight, fullWidth, fullHeight) ?? {
            canvas: [0, 0, 1, 1],
            image: [0, 0, 1, 1],
        };

        let source: GPUImage = base;
        let mapping: SourceMapping = { kx: base.width / fullWidth, ky: base.height / fullHeight, offset: [0, 0] };

        if (pyramid && tiles) {
            const top = pyramid.levels.length - 1;
            const want = levelFor(scale, pyramid.levels);
            if (want < top) {
                const k = pyramid.levels[want].width / fullWidth;
                const apron = Math.ceil((apronFor(1, renderScale, sharpening) / renderScale) * k);
                tiles.request(want, levelRegion(visible.image, pyramid, want, REGION_MARGIN + apron));
            }
            // A finer level than wanted is only worth reading one step down;
            // past that it would shimmer as it shrinks.
            const d = detail;
            if (d && d.level < top && d.level >= want - 1 && contains(d.region, levelRegion(visible.image, pyramid, d.level, 0))) {
                const size = pyramid.levels[d.level];
                source = d.image;
                mapping = {
                    kx: size.width / fullWidth,
                    ky: size.height / fullHeight,
                    offset: [d.region[0], d.region[1]],
                };
            }
        }

        if (source !== bound) {
            renderer.setSource(source);
            bound = source;
        }
        renderer.setView(renderScale, fullWidth, fullHeight);
        renderer.setFrame(planFrame(layout, visible, mapping, sharpening, canvasWidth, canvasHeight));
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
        if (!renderer || !base) return;
        const _ = { ...sliders };
        renderer.setSliders(sliders);
        renderer.requestRender();
    });

    // ── Zoom and pan ──────────────────────────────────────────────────────────

    const cursorOf = (e: { clientX: number; clientY: number }): Point => [e.clientX * dpr, e.clientY * dpr];

    // Zooming out as far as the fit, or past it, goes back to fitting.
    function zoomTo(scale: number, cursor: Point) {
        if (!layout) return;
        if (scale <= layout.fit * 1.0001) {
            zoom = null;
            return;
        }
        const next = Math.min(scale, MAX_ZOOM);
        center = zoomAbout(layout.placement, next, cursor);
        zoom = next;
    }

    // A trackpad pinch arrives as a wheel event with ctrlKey set, as does
    // Ctrl + scroll on a mouse. A notch of a mouse wheel is a doubling.
    function onWheel(e: WheelEvent) {
        if (!layout || showWhole || !e.ctrlKey) return;
        e.preventDefault();
        const delta = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? e.deltaY * 33 : e.deltaY;
        const stops = -Math.max(-100, Math.min(100, delta)) / 100;
        zoomTo(layout.placement.scale * 2 ** stops, cursorOf(e));
    }

    // WebKit reports a trackpad pinch as gesture events instead.
    type GestureEvent = Event & { scale: number; clientX: number; clientY: number };
    let gestureStart = 1;

    function onGestureStart(e: Event) {
        if (!layout || showWhole) return;
        e.preventDefault();
        gestureStart = layout.placement.scale;
    }

    function onGestureChange(e: Event) {
        if (!layout || showWhole) return;
        e.preventDefault();
        const g = e as GestureEvent;
        zoomTo(gestureStart * g.scale, cursorOf(g));
    }

    let dragFrom: Point | null = $state(null);

    function onPointerDown(e: PointerEvent) {
        if (!panning || e.button !== 0) return;
        dragFrom = cursorOf(e);
        canvas.setPointerCapture(e.pointerId);
    }

    function onPointerMove(e: PointerEvent) {
        if (!dragFrom || !layout) return;
        const to = cursorOf(e);
        const { center: c, scale } = layout.placement;
        center = [c[0] - (to[0] - dragFrom[0]) / scale, c[1] - (to[1] - dragFrom[1]) / scale];
        dragFrom = to;
    }

    function onPointerUp() {
        dragFrom = null;
    }

    $effect(() => {
        if (!canvas) return;
        // Not passive: the browser's own page zoom has to be held off.
        canvas.addEventListener("wheel", onWheel, { passive: false });
        canvas.addEventListener("gesturestart", onGestureStart);
        canvas.addEventListener("gesturechange", onGestureChange);
        return () => {
            canvas.removeEventListener("wheel", onWheel);
            canvas.removeEventListener("gesturestart", onGestureStart);
            canvas.removeEventListener("gesturechange", onGestureChange);
        };
    });
</script>

<div
    bind:this={frame}
    class="preview-frame relative"
    style={layout
        ? `width: ${layout.frame.width / dpr}px; height: ${layout.frame.height / dpr}px;`
        : "width: 0; height: 0;"}
>
    <canvas
        bind:this={canvas}
        class:panning
        class:dragging={dragFrom !== null}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={onPointerUp}
    ></canvas>
    {#if layout}
        {@render overlay?.()}
    {/if}
</div>

<style>
    .preview-frame {
        max-width: 100%;
        max-height: 100%;
    }

    /* The whole window, so a zoomed image runs on under the panels, which are
       lifted above it. Premultiplied, so wherever the render leaves the image
       the backdrop shows through. */
    canvas {
        position: fixed;
        left: 0;
        top: 0;
        width: 100vw;
        height: 100vh;
        display: block;
    }

    canvas.panning {
        cursor: grab;
    }

    canvas.dragging {
        cursor: grabbing;
    }
</style>
