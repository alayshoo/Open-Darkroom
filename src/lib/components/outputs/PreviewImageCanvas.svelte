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
    import { loupe } from "$lib/gpu/loupe.svelte";
    import { cropView, type Crop } from "$lib/types/crop";
    import { placeRetouch, type Retouch } from "$lib/types/retouch";
    import { createRetouchLayer, type RetouchLayer } from "$lib/gpu/retouchLayer";
    import { findSource } from "$lib/gpu/retouchSearch";
    import { retouchTool } from "$lib/retouch/retouchTool.svelte";
    import { isTextField } from "$lib/config/keyboardShortcuts";
    import RetouchBrush from "./RetouchBrush.svelte";
    import {
        MAX_ZOOM,
        apply,
        canvasToImage,
        canvasToOutput,
        clampCenter,
        cropLayout,
        invert,
        insideCrop,
        loupeCenter,
        loupeFrame,
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
        retouch = [],
        onretouch,
        retouchReady = $bindable(false),
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
        // The heal and clone strokes baked into the source.
        retouch?: Retouch[];
        // A stroke painted with the brush, ready to be added.
        onretouch?: (stroke: Retouch) => void;
        // Whether the brushes can paint: only once every pixel has arrived.
        retouchReady?: boolean;
        // Drawn over the frame, sized to it.
        overlay?: Snippet;
    } = $props();

    // Get the shared GPU session from layout context
    const gpu = getGPU()!;

    let canvas: HTMLCanvasElement;
    let frame: HTMLDivElement;
    let renderer: Renderer | null = $state(null);
    let retouchLayer: RetouchLayer | null = null;
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
            retouchLayer = createRetouchLayer(gpu.device);
            renderer = createRenderer(gpu, canvasLink);
        });

        return () => {
            destroyed = true;
            loading++;
            loupe.status = "none";
            source?.texture.destroy();
            renderer?.destroy();
            renderer = null;
            retouchLayer?.destroy();
            retouchLayer = null;
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

        // A new image starts the loupe over at its centre.
        untrack(() => {
            savedView = null;
            loupe.picking = false;
            loupe.position = null;
        });

        const preview = uploadRawPixelsToGPU(gpu.device, payload.pixels, payload.width, payload.height, true);
        untrack(() => source)?.texture.destroy();
        source = preview;
        localRenderer.loadImage(preview, payload.srcToWorking);
        localRenderer.requestRender();

        // An image no larger than the preview already has every pixel.
        if (payload.width === payload.fullWidth && payload.height === payload.fullHeight) {
            previewDebug.stream = null;
            loupe.status = "ready";
            retouchReady = true;
            return;
        }
        loupe.status = "loading";
        retouchReady = false;

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
                loupe.status = "ready";
                retouchReady = true;
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
        renderer.setGamutMask(colorSpaceDebug.activeMask);
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

    // The strokes are baked into whichever source is showing, so the preview
    // carries them before the full resolution arrives, and again once it has.
    $effect(() => {
        if (!renderer || !source || !retouchLayer) return;
        if (retouchLayer.sync(source, retouch)) {
            renderer.sourceChanged();
            renderer.requestRender();
        }
        handedOver = null;
        if (untrack(() => stroke?.live)) queuePaint();
    });

    // ── The loupe ─────────────────────────────────────────────────────────────

    // The crop's output, and the point of it the loupe is centred on.
    let loupeView = $derived.by(() => {
        if (!imagePayload || loupe.size <= 0) return null;
        const view = cropView(crop, imagePayload.fullWidth, imagePayload.fullHeight);
        return { view, center: loupeCenter(loupe.position, view, loupe.size) };
    });

    $effect(() => {
        if (!renderer || !imagePayload) return;
        const target = loupe.status === "ready" ? loupe.target : null;
        const placed = loupeView;
        const frame =
            target && placed
                ? loupeFrame(placed.view, placed.center, target.size, imagePayload.fullWidth, imagePayload.fullHeight)
                : null;
        renderer.setLoupe(target?.link ?? null, frame);
        renderer.requestRender();
    });

    // A position the applied crop no longer reaches goes back to its centre.
    $effect(() => {
        if (showWhole || !imagePayload) return;
        const view = cropView(crop, imagePayload.fullWidth, imagePayload.fullHeight);
        untrack(() => {
            if (loupe.position && !insideCrop(loupe.position, view)) loupe.position = null;
        });
    });

    // Placing the loupe shows the whole crop, and puts back the view it
    // replaced once done. The crop tool takes the preview over instead.
    let savedView: { zoom: number | null; center: Point | null } | null = null;

    $effect(() => {
        const picking = loupe.picking;
        untrack(() => {
            if (picking && !savedView) {
                savedView = { zoom, center };
                zoom = null;
            } else if (!picking && savedView) {
                if (!showWhole) {
                    center = savedView.center;
                    zoom = savedView.zoom;
                }
                savedView = null;
            }
        });
    });

    $effect(() => {
        if (showWhole) loupe.picking = false;
    });

    // Where the loupe's square sits on the frame, in CSS pixels.
    let loupeSquare = $derived.by(() => {
        if (!loupe.picking || !layout || !loupeView) return null;
        const { view, center } = loupeView;
        const k = layout.placement.scale / dpr;
        const w = Math.min(loupe.size, view.width);
        const h = Math.min(loupe.size, view.height);
        return { left: (center[0] - w / 2) * k, top: (center[1] - h / 2) * k, width: w * k, height: h * k };
    });

    // The pointer's offset from the loupe's centre, in output pixels, while
    // its square is being dragged.
    let loupeGrab: Point | null = $state(null);

    function placeLoupe(cursor: Point) {
        if (!layout || !loupeView || !loupeGrab) return;
        const { view } = loupeView;
        const [x, y] = apply(canvasToOutput(layout.placement), cursor);
        const target = clampCenter([x + loupeGrab[0], y + loupeGrab[1]], view);
        loupe.position = apply(view, loupeCenter(apply(view, target), view, loupe.size));
    }

    function startLoupeDrag(cursor: Point) {
        if (!layout || !loupeView) return;
        const { center } = loupeView;
        const [x, y] = apply(canvasToOutput(layout.placement), cursor);
        const half = loupe.size / 2;
        const onSquare = Math.abs(x - center[0]) <= half && Math.abs(y - center[1]) <= half;
        loupeGrab = onSquare ? [center[0] - x, center[1] - y] : [0, 0];
        placeLoupe(cursor);
    }

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
            if (gestureStart !== null || loupe.picking) return;
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
        if (loupe.picking) return;
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
        if (e.button !== 0) return;
        if (loupe.picking) {
            startLoupeDrag(cursorOf(e));
            canvas.setPointerCapture(e.pointerId);
            return;
        }
        if (brushing && !spaceHeld) {
            startBrush(e);
            return;
        }
        if (!panning) return;
        dragFrom = cursorOf(e);
        canvas.setPointerCapture(e.pointerId);
    }

    function onPointerMove(e: PointerEvent) {
        hover = cursorOf(e);
        if (loupeGrab) {
            placeLoupe(cursorOf(e));
            return;
        }
        if (resizing || stroke) {
            moveBrush(e);
            return;
        }
        if (!dragFrom) return;
        const to = cursorOf(e);
        panBy(to[0] - dragFrom[0], to[1] - dragFrom[1]);
        dragFrom = to;
    }

    function onPointerUp() {
        dragFrom = null;
        loupeGrab = null;
        // An Alt press that never moved far enough to size the brush picks a source.
        if (resizing && !resizing.moved && onImage(resizing.point)) retouchTool.pickSource(resizing.point);
        resizing = null;
        if (stroke) finishStroke();
    }

    // ── The heal and clone brushes ────────────────────────────────────────────

    // CSS pixels of Alt + drag per percent of feather.
    const RESIZE_PX_PER_FEATHER = 2;
    // CSS pixels an Alt press may wander and still count as a click.
    const CLICK_SLOP_PX = 3;

    let brushing = $derived(retouchTool.mode !== null && retouchReady && !showWhole && !loupe.picking);
    let spaceHeld = $state(false);
    let hover = $state<Point | null>(null);

    // A stroke being painted, in image pixels, and where it takes its pixels
    // from, or null to find out once it is done. A stroke whose source is
    // known is painted into the image as it is drawn.
    let stroke = $state<{ points: Point[]; offset: Point | null; live: boolean } | null>(null);
    // A live stroke just finished, kept painted until the strokes synced take it.
    let handedOver: Retouch | null = null;
    let painting = false;
    let paintQueued = false;
    // Strokes waiting on their source to be found.
    let pending = $state<{ id: number; points: Point[]; radius: number }[]>([]);
    let pendingId = 0;
    let queue: Promise<void> = Promise.resolve();

    // Alt + drag: across sizes the brush, up softens it. Alt + click picks a source.
    let resizing = $state<{ at: Point; point: Point; radius: number; feather: number; moved: boolean } | null>(null);

    let toImage = $derived(layout ? canvasToImage(layout.placement) : null);
    let fromImage = $derived(toImage ? invert(toImage) : null);

    function onImage([x, y]: Point): boolean {
        return !!imagePayload && x >= 0 && y >= 0 && x < imagePayload.fullWidth && y < imagePayload.fullHeight;
    }

    function startBrush(e: PointerEvent) {
        if (!toImage || !imagePayload) return;
        const cursor = cursorOf(e);
        const point = apply(toImage, cursor);
        canvas.setPointerCapture(e.pointerId);
        if (e.altKey) {
            resizing = { at: cursor, point, radius: retouchTool.radius, feather: retouchTool.feather, moved: false };
            return;
        }
        if (!onImage(point)) return;
        const offset = retouchTool.offsetFor(point);
        // Behind a stroke still finding its source, it waits its turn as an outline.
        stroke = { points: [point], offset, live: offset !== null && pending.length === 0 };
        if (stroke.live) queuePaint();
    }

    function moveBrush(e: PointerEvent) {
        if (resizing) {
            const dx = (e.clientX * dpr - resizing.at[0]) / dpr;
            const dy = (e.clientY * dpr - resizing.at[1]) / dpr;
            if (!resizing.moved && Math.hypot(dx, dy) < CLICK_SLOP_PX) return;
            resizing.moved = true;
            retouchTool.setRadius(resizing.radius + (dx * dpr) / (layout?.placement.scale ?? 1));
            retouchTool.setFeather(resizing.feather - dy / RESIZE_PX_PER_FEATHER);
            return;
        }
        if (!stroke || !toImage) return;
        const point = apply(toImage, cursorOf(e));
        const last = stroke.points[stroke.points.length - 1];
        const spacing = Math.max(0.5, retouchTool.radius * 0.15);
        if (Math.hypot(point[0] - last[0], point[1] - last[1]) >= spacing) {
            stroke.points.push(point);
            if (stroke.live) queuePaint();
        }
    }

    function cancelStroke() {
        if (stroke?.live) queuePaint();
        stroke = null;
        resizing = null;
    }

    /** The stroke as it is stored, from image pixels. */
    function strokeRetouch(points: Point[], offset: Point | null): Retouch {
        const { fullWidth, fullHeight } = imagePayload!;
        return {
            mode: retouchTool.mode ?? "heal",
            points: points.map(([x, y]) => [x / fullWidth, y / fullHeight]),
            radius: retouchTool.radius / Math.max(fullWidth, fullHeight),
            feather: retouchTool.feather / 100,
            opacity: retouchTool.opacity / 100,
            offset: offset ? [offset[0] / fullWidth, offset[1] / fullHeight] : [0, 0],
        };
    }

    function queuePaint() {
        if (paintQueued) return;
        paintQueued = true;
        requestAnimationFrame(paintStroke);
    }

    // At most once a frame, and not before the GPU is done with the last one.
    function paintStroke() {
        paintQueued = false;
        if (painting || !retouchLayer || !renderer || !source || !imagePayload) return;
        const next = stroke?.live ? strokeRetouch(stroke.points, stroke.offset) : handedOver;
        if (!retouchLayer.paint(source, next)) return;
        renderer.sourceChanged();
        renderer.requestRender();
        painting = true;
        gpu.device.queue.onSubmittedWorkDone().then(() => {
            painting = false;
            queuePaint();
        });
    }

    // Strokes are added in the order they were painted, however long each
    // takes to find its source.
    function finishStroke() {
        const done = stroke;
        stroke = null;
        if (!done || !imagePayload || !onretouch) {
            if (done?.live) queuePaint();
            return;
        }
        const base = strokeRetouch(done.points, done.offset);
        if (done.live) {
            handedOver = base;
            onretouch(base);
            return;
        }
        const radius = retouchTool.radius;
        const id = ++pendingId;
        pending = [...pending, { id, points: done.points, radius }];
        const image = source;
        const emit = onretouch;
        // A stroke belongs to the image it was painted on, so one opened since drops it.
        const opened = imagePayload;
        const stillOpen = () => imagePayload === opened && source === image;

        queue = queue.then(async () => {
            try {
                if (!stillOpen()) return;
                let finished = base;
                if (!done.offset && image) {
                    const found = await findSource(gpu.device, image, placeRetouch(base, image.width, image.height));
                    if (!found || !stillOpen()) return;
                    finished = { ...base, offset: [found[0] / image.width, found[1] / image.height] };
                }
                emit(finished);
            } catch (e) {
                console.error("Failed to find a source for the stroke:", e);
            } finally {
                pending = pending.filter((p) => p.id !== id);
            }
        });
    }

    const SPACE_CONTROLS = "button, a[href], select, summary, [role='button'], [role='checkbox'], [role='switch'], [role='menuitem'], [role='tab']";

    function onBrushKey(e: KeyboardEvent) {
        if (!brushing || isTextField(e.target)) return;
        if (e.code === "Space") {
            // A focused control keeps Space for itself.
            if (e.target instanceof Element && e.target.closest(SPACE_CONTROLS)) {
                spaceHeld = false;
                return;
            }
            spaceHeld = e.type === "keydown";
            e.preventDefault();
        } else if (e.key === "Alt" && e.type === "keydown") {
            // Stops the window menu taking the key on Windows.
            e.preventDefault();
        } else if (e.key === "Escape" && e.type === "keydown" && (stroke || resizing)) {
            cancelStroke();
            e.preventDefault();
            e.stopImmediatePropagation();
        }
    }

    $effect(() => {
        if (!brushing) {
            cancelStroke();
            spaceHeld = false;
        }
    });

    const toCss = (p: Point): Point => {
        const [x, y] = apply(fromImage!, p);
        return [x / dpr, y / dpr];
    };

    // The brush, in CSS pixels, while it is over the image.
    let brushView = $derived.by(() => {
        if (!brushing || !fromImage || !toImage || !layout || !imagePayload) return null;
        const scale = layout.placement.scale / dpr;
        const radius = retouchTool.radius * scale;
        const anchor = resizing?.at ?? (spaceHeld || dragFrom ? null : hover);
        const at = anchor ? apply(toImage, anchor) : null;
        const offset = stroke ? stroke.offset : retouchTool.offset;
        const drawing = stroke && !stroke.live ? [{ points: stroke.points, radius: retouchTool.radius }] : [];
        const strokes = [...pending, ...drawing];
        return {
            cursor: anchor ? ([anchor[0] / dpr, anchor[1] / dpr] as Point) : null,
            radius,
            feather: retouchTool.feather / 100,
            source: at && offset ? toCss([at[0] + offset[0], at[1] + offset[1]]) : null,
            picked: retouchTool.pickedSource ? toCss(retouchTool.pickedSource) : null,
            strokes: strokes.map((s) => ({ points: s.points.map(toCss), radius: s.radius * scale })),
        };
    });

    $effect(() => {
        if (!canvas) return;
        // Not passive: the browser's own page zoom has to be held off.
        const active = { passive: false };
        canvas.addEventListener("wheel", onWheel, active);
        window.addEventListener("gesturestart", onGestureStart, active);
        window.addEventListener("gesturechange", onGestureChange, active);
        window.addEventListener("gestureend", onGestureEnd, active);
        window.addEventListener("pointermove", onWindowPointer);
        window.addEventListener("keydown", onBrushKey, true);
        window.addEventListener("keyup", onBrushKey, true);
        const releaseSpace = () => (spaceHeld = false);
        window.addEventListener("blur", releaseSpace);
        return () => {
            window.removeEventListener("keydown", onBrushKey, true);
            window.removeEventListener("keyup", onBrushKey, true);
            window.removeEventListener("blur", releaseSpace);
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
        class:panning={panning && (!brushing || spaceHeld)}
        class:dragging={dragFrom !== null || loupeGrab !== null}
        class:placing={loupe.picking}
        class:brushing={brushing && !spaceHeld && !dragFrom}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={() => {
            cancelStroke();
            onPointerUp();
        }}
        onpointerleave={() => (hover = null)}
        oncontextmenu={(e) => {
            if (brushing) e.preventDefault();
        }}
    ></canvas>
    {#if layout}
        {@render overlay?.()}
    {/if}
    {#if brushView}
        <RetouchBrush {...brushView} />
    {/if}
    {#if loupeSquare}
        <div
            class="loupe-square"
            style="left: {loupeSquare.left}px; top: {loupeSquare.top}px; width: {loupeSquare.width}px; height: {loupeSquare.height}px;"
        ></div>
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

    canvas.placing {
        cursor: crosshair;
    }

    canvas.brushing {
        cursor: none;
    }

    canvas.dragging {
        cursor: grabbing;
    }

    .loupe-square {
        position: absolute;
        border: 2px solid var(--loupeAccent);
        box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
        pointer-events: none;
    }
</style>
