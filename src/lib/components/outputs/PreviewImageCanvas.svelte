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
    import { loadFullResolution } from "$lib/gpu/fullResolution";
    import { uploadRawPixelsToGPU } from "$lib/gpu/gpuTextureUpload";
    import { createRenderer } from "$lib/gpu/renderer";
    import { fetchFullResolutionRows } from "$lib/utils/openImage";
    import { type ImagePayload } from "$lib/types/imagePayload";
    import { type Sliders } from "$lib/types/imgParameters";
    import { proofing } from "$lib/gpu/proofing.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { previewDebug } from "$lib/gpu/previewDebug.svelte";
    import { type Crop } from "$lib/types/crop";
    import {
        MAX_ZOOM,
        cropLayout,
        viewFrame,
        wholeLayout,
        zoomAbout,
        type Point,
        type Slot,
    } from "$lib/utils/viewport";

    // Props
    let {
        sliders,
        curveLut,
        imagePayload,
        crop,
        showWhole = false,
        zoom = $bindable(null),
        zoomable = $bindable(false),
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
        // Whether 1:1 is any closer than the fit.
        zoomable?: boolean;
        // Drawn over the frame, sized to it.
        overlay?: Snippet;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let frame: HTMLDivElement;
    let renderer: Renderer | null = $state(null);
    let canvasLink: GPUCanvasLink | null = $state(null);
    // The texture the renderer is reading: the preview from the open payload,
    // then the full-resolution image once it has streamed in.
    let source: GPUImage | null = $state.raw(null);
    // Bumped for every image, so a stream still running for the last one
    // knows to stop.
    let loading = 0;
    // The output point held over the middle of the slot while zoomed.
    let center = $state<Point | null>(null);

    // The canvas covers the whole window, so a zoomed image runs under the
    // panels; the slot is only where the image is centred and fitted.
    let dpr = $state(1);
    let canvasWidth = $state(1);
    let canvasHeight = $state(1);
    let slot = $state.raw<Slot | null>(null);

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

    let panning = $derived(zoom !== null && !showWhole);

    $effect(() => {
        if (zoom === null) center = null;
    });

    const isFit = (scale: number, fit: number) => scale <= fit * 1.0001;

    // A zoom no closer than the fit is the fit, however it was asked for.
    $effect(() => {
        if (zoom !== null && layout && !showWhole && isFit(zoom, layout.fit)) zoom = null;
    });

    $effect(() => {
        zoomable = !!layout && !showWhole && !isFit(1, layout.fit);
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
            loading++;
            source?.texture.destroy();
            renderer?.destroy();
            renderer = null;
            canvasLink = null;
            source = null;
        };
    });

    // A new image: the preview from the payload goes up at once, and the full
    // resolution streams in behind it and takes over once it is complete.
    $effect(() => {
        if (!renderer || !imagePayload) return;
        const payload = imagePayload;
        const localRenderer = renderer;
        const load = ++loading;

        const preview = uploadRawPixelsToGPU(gpu.device, payload.pixels, payload.width, payload.height, true);
        untrack(() => source)?.texture.destroy();
        source = preview;
        localRenderer.loadImage(preview, payload.srcToWorking);
        localRenderer.requestRender();

        // An image no larger than the preview already has every pixel.
        if (payload.width === payload.fullWidth && payload.height === payload.fullHeight) {
            previewDebug.stream = null;
            return;
        }

        const started = performance.now();
        previewDebug.stream = { rows: 0, total: payload.fullHeight, done: false };
        const progress = (rows: number, total: number) => {
            if (load === loading) previewDebug.stream = { rows, total, done: rows >= total };
        };
        const fetchRows = (maxEdge: number, y: number, rows: number) =>
            fetchFullResolutionRows(payload.imageId, maxEdge, y, rows);
        loadFullResolution(gpu.device, payload.fullWidth, fetchRows, () => load !== loading, progress)
            .then((full) => {
                if (!full) return;
                if (load !== loading) {
                    full.texture.destroy();
                    return;
                }
                console.log(
                    `Full resolution ${full.width}×${full.height} on the GPU in ${Math.round(performance.now() - started)} ms`,
                );
                localRenderer.setSource(full);
                localRenderer.requestRender();
                preview.texture.destroy();
                source = full;
            })
            .catch((e) => {
                if (load === loading) console.error("Failed to load the full-resolution image:", e);
            });
    });

    // Where the image sits on the canvas. Moving or zooming it is cheap: the
    // renderer only re-runs the develop chain when what it holds runs out.
    $effect(() => {
        if (!renderer || !source || !layout || !imagePayload) return;

        if (canvas.width !== canvasWidth || canvas.height !== canvasHeight) {
            canvas.width = canvasWidth;
            canvas.height = canvasHeight;
            void linkCanvas2GPU(canvas, gpu, "premultiplied");
        }

        const { fullWidth, fullHeight } = imagePayload;
        renderer.setFrame(viewFrame(layout, canvasWidth, canvasHeight, fullWidth, fullHeight));
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
        if (!renderer || !source) return;
        const _ = { ...sliders };
        renderer.setSliders(sliders);
        renderer.requestRender();
    });

    // ── Zoom and pan ──────────────────────────────────────────────────────────

    const cursorOf = (e: { clientX: number; clientY: number }): Point => [e.clientX * dpr, e.clientY * dpr];

    // Where the pointer last was, for input that does not say where it is.
    let pointer: Point | null = null;
    const onWindowPointer = (e: PointerEvent) => (pointer = cursorOf(e));

    // Zooming out as far as the fit, or past it, goes back to fitting.
    function zoomTo(scale: number, cursor: Point) {
        if (!layout || !Number.isFinite(scale) || !cursor.every(Number.isFinite)) return;
        if (isFit(scale, layout.fit)) {
            zoom = null;
            return;
        }
        const next = Math.min(scale, MAX_ZOOM);
        center = zoomAbout(layout.placement, next, cursor);
        zoom = next;
    }

    function panBy(dx: number, dy: number) {
        if (!layout) return;
        const { center: c, scale } = layout.placement;
        center = [c[0] - dx / scale, c[1] - dy / scale];
    }

    // WebKit reports a trackpad pinch as gesture events, which take over from
    // the wheel for as long as the pinch lasts. They are heard on the window,
    // since WebKit may not aim them at the canvas, and may carry no position.
    type GestureEvent = Event & { scale: number; clientX?: number; clientY?: number };
    let gestureStart: number | null = null;

    function gestureCursor(g: GestureEvent): Point {
        if (Number.isFinite(g.clientX) && Number.isFinite(g.clientY)) {
            return cursorOf(g as { clientX: number; clientY: number });
        }
        return pointer ?? layout!.placement.anchor;
    }

    // A gesture over the panels is theirs; one over the image, or aimed at no
    // element in particular, is the preview's.
    function overImage(e: Event): boolean {
        const target = e.target;
        if (!(target instanceof Node)) return true;
        return target === document.documentElement || target === document.body || frame.contains(target);
    }

    // A trackpad pinch elsewhere arrives as a wheel event with ctrlKey set, as
    // does Ctrl + scroll on a mouse; a notch of a mouse wheel is a doubling.
    // Any other scroll pans a zoomed image.
    function onWheel(e: WheelEvent) {
        previewDebug.noteInput("wheel", `${e.ctrlKey ? "ctrl " : ""}dx ${e.deltaX.toFixed(1)} dy ${e.deltaY.toFixed(1)}`);
        if (!layout || showWhole) return;
        const lines = e.deltaMode === WheelEvent.DOM_DELTA_LINE ? 33 : 1;
        if (e.ctrlKey) {
            e.preventDefault();
            if (gestureStart !== null) return;
            const stops = -Math.max(-100, Math.min(100, e.deltaY * lines)) / 100;
            zoomTo(layout.placement.scale * 2 ** stops, cursorOf(e));
        } else if (panning) {
            e.preventDefault();
            panBy(-e.deltaX * lines * dpr, -e.deltaY * lines * dpr);
        }
    }

    function onGestureStart(e: Event) {
        previewDebug.noteInput("gesturestart", describeGesture(e as GestureEvent));
        if (!layout || showWhole || !overImage(e)) return;
        e.preventDefault();
        gestureStart = layout.placement.scale;
    }

    function onGestureChange(e: Event) {
        const g = e as GestureEvent;
        previewDebug.noteInput("gesturechange", describeGesture(g));
        if (!layout || showWhole || gestureStart === null) return;
        e.preventDefault();
        zoomTo(gestureStart * g.scale, gestureCursor(g));
    }

    function onGestureEnd(e: Event) {
        previewDebug.noteInput("gestureend", describeGesture(e as GestureEvent));
        if (gestureStart !== null) e.preventDefault();
        gestureStart = null;
    }

    const describeGesture = (g: GestureEvent) =>
        `scale ${Number(g.scale).toFixed(3)} at ${g.clientX ?? "?"},${g.clientY ?? "?"}`;

    let dragFrom: Point | null = $state(null);

    function onPointerDown(e: PointerEvent) {
        if (!panning || e.button !== 0) return;
        dragFrom = cursorOf(e);
        canvas.setPointerCapture(e.pointerId);
    }

    function onPointerMove(e: PointerEvent) {
        if (!dragFrom) return;
        const to = cursorOf(e);
        panBy(to[0] - dragFrom[0], to[1] - dragFrom[1]);
        dragFrom = to;
    }

    function onPointerUp() {
        dragFrom = null;
    }

    $effect(() => {
        if (!canvas) return;
        // Not passive: the browser's own page zoom has to be held off.
        const active = { passive: false };
        canvas.addEventListener("wheel", onWheel, active);
        window.addEventListener("gesturestart", onGestureStart, active);
        window.addEventListener("gesturechange", onGestureChange, active);
        window.addEventListener("gestureend", onGestureEnd, active);
        window.addEventListener("pointermove", onWindowPointer);
        return () => {
            canvas.removeEventListener("wheel", onWheel);
            window.removeEventListener("gesturestart", onGestureStart);
            window.removeEventListener("gesturechange", onGestureChange);
            window.removeEventListener("gestureend", onGestureEnd);
            window.removeEventListener("pointermove", onWindowPointer);
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
