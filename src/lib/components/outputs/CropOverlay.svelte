<!-- src/lib/components/outputs/CropOverlay.svelte -->
<script lang="ts">
    import { MAX_ANGLE, cropInside, fitCrop, type Crop } from "$lib/types/crop";

    // Laid over the whole turned and rotated frame, so every position here is a
    // fraction of that frame and maps straight onto the crop's own units.
    let {
        crop = $bindable(),
        straightening = $bindable(false),
        imageWidth,
        imageHeight,
    }: {
        crop: Crop;
        // While set, a drag draws a line to level instead of moving anything.
        straightening?: boolean;
        // The source's size, before any turn. Only its proportions matter.
        imageWidth: number;
        imageHeight: number;
    } = $props();

    // Smallest the crop may get along either axis, as a fraction of the frame.
    const MIN_EXTENT = 0.02;
    // Shorter straighten lines than this, in CSS pixels, are taken as a slip.
    const MIN_LINE = 8;

    interface Rect {
        x0: number;
        y0: number;
        x1: number;
        y1: number;
    }

    // Which sides a drag moves. All four is a move of the whole box.
    interface Sides {
        l: boolean;
        r: boolean;
        t: boolean;
        b: boolean;
    }

    const MOVE: Sides = { l: true, r: true, t: true, b: true };

    const HANDLES: { id: string; sides: Sides }[] = [
        { id: "nw", sides: { l: true, r: false, t: true, b: false } },
        { id: "n", sides: { l: false, r: false, t: true, b: false } },
        { id: "ne", sides: { l: false, r: true, t: true, b: false } },
        { id: "e", sides: { l: false, r: true, t: false, b: false } },
        { id: "se", sides: { l: false, r: true, t: false, b: true } },
        { id: "s", sides: { l: false, r: false, t: false, b: true } },
        { id: "sw", sides: { l: true, r: false, t: false, b: true } },
        { id: "w", sides: { l: true, r: false, t: false, b: false } },
    ];

    let frame: HTMLDivElement;

    let rect: Rect = $derived({
        x0: crop.cx - crop.w / 2,
        y0: crop.cy - crop.h / 2,
        x1: crop.cx + crop.w / 2,
        y1: crop.cy + crop.h / 2,
    });

    type Drag =
        | { kind: "resize"; sides: Sides; pointerX: number; pointerY: number; start: Crop }
        | { kind: "rotate"; pivotX: number; pivotY: number; startBearing: number; start: Crop }
        | { kind: "line" };

    let drag: Drag | null = null;
    let mode = $state<"idle" | "resize" | "rotate" | "line">("idle");
    // The cursor a drag started with, held over the whole window until it
    // ends, so passing over another part of the overlay never swaps it.
    let dragCursor = $state<string | null>(null);

    function cursorFor(sides: Sides): string {
        const { l, r, t, b } = sides;
        if (l && r && t && b) return "move";
        if ((l && t) || (r && b)) return "nwse-resize";
        if ((r && t) || (l && b)) return "nesw-resize";
        return t || b ? "ns-resize" : "ew-resize";
    }
    // The straighten line, in fractions of the frame.
    let line: { x0: number; y0: number; x1: number; y1: number } | null = $state(null);

    // On the image, and within the upright frame the overlay is laid out on.
    const inside = (c: Crop) =>
        cropInside(c, imageWidth, imageHeight) &&
        c.cx - c.w / 2 >= -1e-9 &&
        c.cy - c.h / 2 >= -1e-9 &&
        c.cx + c.w / 2 <= 1 + 1e-9 &&
        c.cy + c.h / 2 <= 1 + 1e-9;

    // The furthest a change can go from a crop on the image towards `to`
    // before a corner leaves it.
    function limited(from: Crop, to: Crop): Crop {
        if (inside(to)) return to;
        const at = (t: number): Crop => ({
            ...to,
            cx: from.cx + (to.cx - from.cx) * t,
            cy: from.cy + (to.cy - from.cy) * t,
            w: from.w + (to.w - from.w) * t,
            h: from.h + (to.h - from.h) * t,
        });
        let lo = 0;
        let hi = 1;
        for (let i = 0; i < 20; i++) {
            const mid = (lo + hi) / 2;
            if (inside(at(mid))) lo = mid;
            else hi = mid;
        }
        return at(lo);
    }

    function fromRect(r: Rect, base: Crop): Crop {
        return {
            ...base,
            cx: (r.x0 + r.x1) / 2,
            cy: (r.y0 + r.y1) / 2,
            w: r.x1 - r.x0,
            h: r.y1 - r.y0,
        };
    }

    function toRect(c: Crop): Rect {
        return { x0: c.cx - c.w / 2, y0: c.cy - c.h / 2, x1: c.cx + c.w / 2, y1: c.cy + c.h / 2 };
    }

    function clamp(v: number, lo: number, hi: number) {
        return Math.min(Math.max(v, lo), hi);
    }

    function fractionOf(e: PointerEvent): [number, number] {
        const bounds = frame.getBoundingClientRect();
        return [(e.clientX - bounds.left) / bounds.width, (e.clientY - bounds.top) / bounds.height];
    }

    function beginLine(e: PointerEvent) {
        const [x, y] = fractionOf(e);
        drag = { kind: "line" };
        line = { x0: x, y0: y, x1: x, y1: y };
        mode = "line";
        dragCursor = "crosshair";
    }

    function beginResize(e: PointerEvent, sides: Sides) {
        if (e.button !== 0) return;
        e.preventDefault();
        e.stopPropagation();
        if (straightening || e.ctrlKey || e.metaKey) return beginLine(e);
        drag = { kind: "resize", sides, pointerX: e.clientX, pointerY: e.clientY, start: { ...crop } };
        mode = "resize";
        dragCursor = cursorFor(sides);
    }

    // Anywhere off the box turns the image about the box's centre.
    function beginRotate(e: PointerEvent) {
        if (e.button !== 0) return;
        e.preventDefault();
        if (straightening || e.ctrlKey || e.metaKey) return beginLine(e);
        const bounds = frame.getBoundingClientRect();
        const pivotX = bounds.left + crop.cx * bounds.width;
        const pivotY = bounds.top + crop.cy * bounds.height;
        drag = {
            kind: "rotate",
            pivotX,
            pivotY,
            startBearing: Math.atan2(e.clientY - pivotY, e.clientX - pivotX),
            start: { ...crop },
        };
        mode = "rotate";
        dragCursor = "var(--rotate-cursor)";
    }

    function move(e: PointerEvent) {
        if (!drag) return;

        if (drag.kind === "line") {
            const [x, y] = fractionOf(e);
            if (line) line = { ...line, x1: x, y1: y };
            return;
        }

        if (drag.kind === "rotate") {
            let turn = Math.atan2(e.clientY - drag.pivotY, e.clientX - drag.pivotX) - drag.startBearing;
            turn = Math.atan2(Math.sin(turn), Math.cos(turn));
            const angle = clamp(drag.start.angle + turn, -MAX_ANGLE, MAX_ANGLE);
            crop = fitCrop({ ...drag.start, angle }, imageWidth, imageHeight);
            return;
        }

        const bounds = frame.getBoundingClientRect();
        const dx = (e.clientX - drag.pointerX) / bounds.width;
        const dy = (e.clientY - drag.pointerY) / bounds.height;
        const start = drag.start;
        const s = toRect(start);
        const { l, r, t, b } = drag.sides;

        // Each axis is limited on its own, so a box pinned on one side can still
        // slide along the other.
        let next: Crop;
        if (l && r && t && b) {
            const across = limited(start, { ...start, cx: start.cx + dx });
            next = limited(across, { ...across, cy: across.cy + dy });
        } else {
            const x = { ...s };
            if (l) x.x0 = clamp(s.x0 + dx, 0, s.x1 - MIN_EXTENT);
            if (r) x.x1 = clamp(s.x1 + dx, s.x0 + MIN_EXTENT, 1);
            const across = limited(start, fromRect(x, start));

            const y = toRect(across);
            if (t) y.y0 = clamp(s.y0 + dy, 0, y.y1 - MIN_EXTENT);
            if (b) y.y1 = clamp(s.y1 + dy, y.y0 + MIN_EXTENT, 1);
            next = limited(across, fromRect(y, across));
        }
        crop = next;
    }

    // A finished line is turned level, or plumb when it is nearer upright.
    function straighten() {
        if (!line) return;
        const bounds = frame.getBoundingClientRect();
        const dx = (line.x1 - line.x0) * bounds.width;
        const dy = (line.y1 - line.y0) * bounds.height;
        if (Math.hypot(dx, dy) < MIN_LINE) return;

        const bearing = Math.atan2(dy, dx);
        const tilt = bearing - Math.round(bearing / (Math.PI / 2)) * (Math.PI / 2);
        const angle = clamp(crop.angle - tilt, -MAX_ANGLE, MAX_ANGLE);
        crop = fitCrop({ ...crop, angle }, imageWidth, imageHeight);
        straightening = false;
    }

    function end() {
        if (drag?.kind === "line") straighten();
        drag = null;
        line = null;
        mode = "idle";
        dragCursor = null;
    }

    const pct = (v: number) => `${v * 100}%`;

    // Thirds at rest; a finer grid to sight along while the image is turning.
    let gridLines = $derived(
        mode === "rotate" ? [1, 2, 3, 4, 5, 6, 7].map((i) => i / 8) : [1 / 3, 2 / 3],
    );

    let degrees = $derived(((crop.angle * 180) / Math.PI).toFixed(1));
</script>

<svelte:window onpointermove={move} onpointerup={end} onpointercancel={end} />

<div
    class="crop-overlay absolute"
    class:dragging={mode !== "idle"}
    class:straightening
    bind:this={frame}
    onpointerdown={beginRotate}
    oncontextmenu={(e) => e.preventDefault()}
    role="presentation"
>
    <div class="rotate-zone"></div>
    {#if dragCursor}
        <div class="drag-shield" style="cursor: {dragCursor};"></div>
    {/if}

    <div
        class="crop-box absolute"
        style="left: {pct(rect.x0)}; top: {pct(rect.y0)}; width: {pct(rect.x1 - rect.x0)}; height: {pct(rect.y1 - rect.y0)};"
        onpointerdown={(e) => beginResize(e, MOVE)}
        role="presentation"
    >
        {#each gridLines as at (at)}
            <div class="grid v" style="left: {pct(at)};"></div>
            <div class="grid h" style="top: {pct(at)};"></div>
        {/each}

        {#each HANDLES as handle (handle.id)}
            <div
                class="handle {handle.id}"
                onpointerdown={(e) => beginResize(e, handle.sides)}
                role="presentation"
            ></div>
        {/each}

        {#if mode === "rotate"}
            <div class="angle-readout">{degrees}°</div>
        {/if}
    </div>

    {#if line}
        <svg class="level-line absolute" width="100%" height="100%">
            <line x1={pct(line.x0)} y1={pct(line.y0)} x2={pct(line.x1)} y2={pct(line.y1)} />
        </svg>
    {/if}
</div>

<style>
    /* Off the box the pointer turns the image, so it shows a curved arrow. */
    .crop-overlay {
        inset: 0;
        touch-action: none;
        user-select: none;
        --rotate-cursor: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'%3E%3Cg fill='none' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M5 15a8 8 0 0 1 14 0M5 15l-1-4M5 15l4-1M19 15l1-4M19 15l-4-1' stroke='black' stroke-width='3.5'/%3E%3Cpath d='M5 15a8 8 0 0 1 14 0M5 15l-1-4M5 15l4-1M19 15l1-4M19 15l-4-1' stroke='white' stroke-width='1.5'/%3E%3C/g%3E%3C/svg%3E") 12 12, grab;
        cursor: var(--rotate-cursor);
    }

    /* Over everything, the side panels included, for as long as a drag runs. */
    .drag-shield {
        position: fixed;
        inset: 0;
        z-index: 100;
    }

    /* Reaches well past the image, so a box filling the frame still leaves
       somewhere to grab. The side panels sit above it and keep their clicks. */
    .rotate-zone {
        position: absolute;
        inset: -100vmax;
    }

    .crop-overlay.straightening,
    .crop-overlay.straightening .crop-box,
    .crop-overlay.straightening .handle {
        cursor: crosshair;
    }

    .crop-box {
        box-sizing: border-box;
        border: 1px solid color-mix(in srgb, var(--color1) 85%, transparent);
        cursor: move;
    }

    /* The grid reads lighter at rest and firms up while anything is dragged. */
    .grid {
        position: absolute;
        background: color-mix(in srgb, var(--color1) 35%, transparent);
        pointer-events: none;
        transition: background 0.15s ease;
    }
    .dragging .grid {
        background: color-mix(in srgb, var(--color1) 60%, transparent);
    }
    .grid.v {
        top: 0;
        bottom: 0;
        width: 1px;
    }
    .grid.h {
        left: 0;
        right: 0;
        height: 1px;
    }

    .angle-readout {
        position: absolute;
        left: 50%;
        top: 10px;
        transform: translateX(-50%);
        padding: 2px 8px;
        border-radius: 6px;
        background: rgba(0, 0, 0, 0.6);
        color: var(--color1);
        font: 500 12px "Figtree", sans-serif;
        font-variant-numeric: tabular-nums;
        pointer-events: none;
    }

    .level-line {
        inset: 0;
        overflow: visible;
        pointer-events: none;
    }
    .level-line line {
        stroke: var(--color1);
        stroke-width: 1.5;
        filter: drop-shadow(0 0 1.5px rgba(0, 0, 0, 0.8));
    }

    /* The hit area is generous; the visible mark is drawn by ::after. */
    .handle {
        position: absolute;
        width: 24px;
        height: 24px;
    }
    .handle::after {
        content: "";
        position: absolute;
        border-color: var(--color1);
        border-style: solid;
        border-width: 0;
        filter: drop-shadow(0 0 1.5px rgba(0, 0, 0, 0.6));
    }

    .nw, .ne, .se, .sw {
        --len: 18px;
    }
    .nw { left: -12px; top: -12px; cursor: nwse-resize; }
    .ne { right: -12px; top: -12px; cursor: nesw-resize; }
    .se { right: -12px; bottom: -12px; cursor: nwse-resize; }
    .sw { left: -12px; bottom: -12px; cursor: nesw-resize; }
    .nw::after { left: 10px; top: 10px; width: var(--len); height: var(--len); border-left-width: 3px; border-top-width: 3px; }
    .ne::after { right: 10px; top: 10px; width: var(--len); height: var(--len); border-right-width: 3px; border-top-width: 3px; }
    .se::after { right: 10px; bottom: 10px; width: var(--len); height: var(--len); border-right-width: 3px; border-bottom-width: 3px; }
    .sw::after { left: 10px; bottom: 10px; width: var(--len); height: var(--len); border-left-width: 3px; border-bottom-width: 3px; }

    .n, .s {
        left: 24px;
        right: 24px;
        width: auto;
        cursor: ns-resize;
    }
    .n { top: -12px; }
    .s { bottom: -12px; }
    .n::after, .s::after {
        left: calc(50% - 10px);
        width: 20px;
        height: 0;
        border-top-width: 3px;
    }
    .n::after { top: 10px; }
    .s::after { bottom: 10px; }

    .e, .w {
        top: 24px;
        bottom: 24px;
        height: auto;
        cursor: ew-resize;
    }
    .e { right: -12px; }
    .w { left: -12px; }
    .e::after, .w::after {
        top: calc(50% - 10px);
        height: 20px;
        width: 0;
        border-left-width: 3px;
    }
    .e::after { right: 10px; }
    .w::after { left: 10px; }
</style>
