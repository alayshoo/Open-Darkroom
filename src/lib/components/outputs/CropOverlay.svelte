<!-- src/lib/components/outputs/CropOverlay.svelte -->
<script lang="ts">
    import type { Crop } from "$lib/types/crop";

    // Laid over the uncropped preview, so every position here is a fraction of
    // the whole frame and maps straight onto the crop's own units.
    let { crop = $bindable() }: { crop: Crop } = $props();

    // Smallest the crop may get along either axis, as a fraction of the frame.
    const MIN_EXTENT = 0.02;

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

    let drag: {
        sides: Sides;
        pointerX: number;
        pointerY: number;
        start: Rect;
    } | null = null;

    let dragging = $state(false);

    function begin(e: PointerEvent, sides: Sides) {
        if (e.button !== 0) return;
        e.preventDefault();
        e.stopPropagation();
        drag = { sides, pointerX: e.clientX, pointerY: e.clientY, start: { ...rect } };
        dragging = true;
    }

    function clamp(v: number, lo: number, hi: number) {
        return Math.min(Math.max(v, lo), hi);
    }

    function move(e: PointerEvent) {
        if (!drag) return;
        const bounds = frame.getBoundingClientRect();
        const dx = (e.clientX - drag.pointerX) / bounds.width;
        const dy = (e.clientY - drag.pointerY) / bounds.height;
        const s = drag.start;
        const { l, r, t, b } = drag.sides;
        let next: Rect;

        if (l && r && t && b) {
            const ox = clamp(dx, -s.x0, 1 - s.x1);
            const oy = clamp(dy, -s.y0, 1 - s.y1);
            next = { x0: s.x0 + ox, y0: s.y0 + oy, x1: s.x1 + ox, y1: s.y1 + oy };
        } else {
            next = { ...s };
            if (l) next.x0 = clamp(s.x0 + dx, 0, s.x1 - MIN_EXTENT);
            if (r) next.x1 = clamp(s.x1 + dx, s.x0 + MIN_EXTENT, 1);
            if (t) next.y0 = clamp(s.y0 + dy, 0, s.y1 - MIN_EXTENT);
            if (b) next.y1 = clamp(s.y1 + dy, s.y0 + MIN_EXTENT, 1);
        }

        crop = {
            cx: (next.x0 + next.x1) / 2,
            cy: (next.y0 + next.y1) / 2,
            w: next.x1 - next.x0,
            h: next.y1 - next.y0,
            angle: crop.angle,
        };
    }

    function end() {
        drag = null;
        dragging = false;
    }

    const pct = (v: number) => `${v * 100}%`;
</script>

<svelte:window onpointermove={move} onpointerup={end} onpointercancel={end} />

<div class="crop-overlay absolute" class:dragging bind:this={frame}>
    <div class="shade" style="left: 0; top: 0; right: 0; height: {pct(rect.y0)};"></div>
    <div class="shade" style="left: 0; bottom: 0; right: 0; top: {pct(rect.y1)};"></div>
    <div
        class="shade"
        style="left: 0; top: {pct(rect.y0)}; width: {pct(rect.x0)}; height: {pct(rect.y1 - rect.y0)};"
    ></div>
    <div
        class="shade"
        style="right: 0; top: {pct(rect.y0)}; left: {pct(rect.x1)}; height: {pct(rect.y1 - rect.y0)};"
    ></div>

    <div
        class="crop-box absolute"
        style="left: {pct(rect.x0)}; top: {pct(rect.y0)}; width: {pct(rect.x1 - rect.x0)}; height: {pct(rect.y1 - rect.y0)};"
        onpointerdown={(e) => begin(e, MOVE)}
        role="presentation"
    >
        <div class="third v" style="left: 33.333%;"></div>
        <div class="third v" style="left: 66.667%;"></div>
        <div class="third h" style="top: 33.333%;"></div>
        <div class="third h" style="top: 66.667%;"></div>

        {#each HANDLES as handle (handle.id)}
            <div
                class="handle {handle.id}"
                onpointerdown={(e) => begin(e, handle.sides)}
                role="presentation"
            ></div>
        {/each}
    </div>
</div>

<style>
    .crop-overlay {
        inset: 0;
        touch-action: none;
        user-select: none;
    }

    .shade {
        position: absolute;
        background: rgba(0, 0, 0, 0.55);
        pointer-events: none;
    }

    .crop-box {
        box-sizing: border-box;
        border: 1px solid color-mix(in srgb, var(--color1) 85%, transparent);
        cursor: move;
    }

    /* Thirds read lighter at rest and firm up while the box is being dragged. */
    .third {
        position: absolute;
        background: color-mix(in srgb, var(--color1) 35%, transparent);
        pointer-events: none;
        transition: background 0.15s ease;
    }
    .dragging .third {
        background: color-mix(in srgb, var(--color1) 60%, transparent);
    }
    .third.v {
        top: 0;
        bottom: 0;
        width: 1px;
    }
    .third.h {
        left: 0;
        right: 0;
        height: 1px;
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
