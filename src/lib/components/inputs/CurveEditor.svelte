<!-- lib/components/inputs/CurveEditor.svelte -->
<script lang="ts">
    import type { Snippet } from "svelte";
    import {
        CURVE_CHANNELS,
        CURVE_COLOURS,
        cloneCurves,
        curveEvaluator,
        curvesEqual,
        isIdentityCurve,
        type CurveChannel,
        type CurvePoint,
        type Curves,
    } from "$lib/types/curves";

    let {
        curves = $bindable(),
        channel = $bindable("master"),
        isRgb,
        histogramCanvas = $bindable(null),
        title,
        onCommit = (_old: Curves, _new: Curves) => {},
    }: {
        curves: Curves;
        // The curve picked for editing. B&W edits the master whatever this holds.
        channel?: CurveChannel;
        // Off in B&W, where only the master curve applies.
        isRgb: boolean;
        // Drawn into by the histogram, behind the curves.
        histogramCanvas?: HTMLCanvasElement | null;
        title?: Snippet;
        onCommit?: (oldCurves: Curves, newCurves: Curves) => void;
    } = $props();


    const LABELS: Record<CurveChannel, string> = {
        master: "RGB",
        red: "Red",
        green: "Green",
        blue: "Blue",
    };

    const SAMPLES = 128;
    const GRID = [0.25, 0.5, 0.75];
    // Pointer distance, in CSS pixels, that grabs a point.
    const HIT_RADIUS = 8;
    // Closest two neighbouring points may sit, as a fraction of the axis.
    const MIN_GAP = 0.01;
    // How far past the edge a dragged point has to go to be removed.
    const REMOVE_DISTANCE = 24;

    const active = $derived<CurveChannel>(isRgb ? channel : "master");
    const visible = $derived<readonly CurveChannel[]>(isRgb ? CURVE_CHANNELS : ["master"]);

    let svg: SVGSVGElement | null = $state(null);
    let size = $state(0);

    let drag: {
        channel: CurveChannel;
        index: number;
        base: CurvePoint[];
        before: Curves;
    } | null = null;
    let dragging = $state(false);

    const paths = $derived.by(() => {
        const out = {} as Record<CurveChannel, string>;
        for (const id of CURVE_CHANNELS) {
            const f = curveEvaluator(curves[id]);
            let d = "";
            for (let i = 0; i <= SAMPLES; i++) {
                const x = i / SAMPLES;
                d += `${i === 0 ? "M" : "L"}${(x * size).toFixed(2)},${((1 - f(x)) * size).toFixed(2)}`;
            }
            out[id] = d;
        }
        return out;
    });

    // Inactive curves first, so the one being edited is drawn on top.
    const drawOrder = $derived([...visible.filter((id) => id !== active), active]);

    function clamp(v: number, lo: number, hi: number) {
        return Math.min(Math.max(v, lo), hi);
    }

    function pointerPosition(e: PointerEvent | MouseEvent) {
        const rect = svg!.getBoundingClientRect();
        const px = e.clientX - rect.left;
        const py = e.clientY - rect.top;
        const outside = Math.max(-px, px - rect.width, -py, py - rect.height, 0);
        return { x: px / rect.width, y: 1 - py / rect.height, px, py, outside };
    }

    function hitTest(px: number, py: number): number {
        const points = curves[active];
        let best = -1;
        let bestDistance = HIT_RADIUS;
        points.forEach((p, i) => {
            const distance = Math.hypot(p.x * size - px, (1 - p.y) * size - py);
            if (distance <= bestDistance) {
                best = i;
                bestDistance = distance;
            }
        });
        return best;
    }

    function snapshot(): Curves {
        return cloneCurves($state.snapshot(curves) as Curves);
    }

    function handlePointerDown(e: PointerEvent) {
        if (e.button !== 0 || !svg) return;
        const pos = pointerPosition(e);
        const before = snapshot();
        let points = before[active].map((p) => ({ ...p }));
        let index = hitTest(pos.px, pos.py);

        if (index < 0) {
            const x = clamp(pos.x, 0, 1);
            const at = points.findIndex((p) => p.x > x);
            if (at <= 0) return;
            if (x - points[at - 1].x < MIN_GAP || points[at].x - x < MIN_GAP) return;
            points.splice(at, 0, { x, y: clamp(pos.y, 0, 1) });
            curves[active] = points;
            index = at;
        }

        drag = { channel: active, index, base: points, before };
        dragging = true;
        svg.setPointerCapture(e.pointerId);
        e.preventDefault();
    }

    function handlePointerMove(e: PointerEvent) {
        if (!drag) return;
        const pos = pointerPosition(e);
        const points = drag.base.map((p) => ({ ...p }));
        const i = drag.index;
        const isEnd = i === 0 || i === points.length - 1;

        if (!isEnd && pos.outside > REMOVE_DISTANCE) {
            points.splice(i, 1);
        } else {
            if (!isEnd) {
                points[i].x = clamp(pos.x, points[i - 1].x + MIN_GAP, points[i + 1].x - MIN_GAP);
            }
            points[i].y = clamp(pos.y, 0, 1);
        }
        curves[drag.channel] = points;
    }

    function endDrag() {
        if (!drag) return;
        const { before } = drag;
        drag = null;
        dragging = false;
        const after = snapshot();
        if (!curvesEqual(before, after)) onCommit(before, after);
    }

    function handleDoubleClick(e: MouseEvent) {
        if (!svg) return;
        const pos = pointerPosition(e);
        const index = hitTest(pos.px, pos.py);
        if (index <= 0 || index >= curves[active].length - 1) return;

        const before = snapshot();
        curves[active] = before[active].filter((_, i) => i !== index);
        onCommit(before, snapshot());
    }
</script>

<div class="flex flex-col">
    <div class="flex flex-row items-center justify-between pb-1.5 pr-1.5">
        {@render title?.()}
        <div class="channels flex flex-row items-center gap-2" role="radiogroup">
            {#each CURVE_CHANNELS as option (option)}
                <button
                    class="channel"
                    class:selected={option === active}
                    class:adjusted={!isIdentityCurve(curves[option])}
                    style="--swatch: {CURVE_COLOURS[option]}"
                    role="radio"
                    aria-checked={option === active}
                    aria-label={LABELS[option]}
                    title={LABELS[option]}
                    disabled={!isRgb && option !== "master"}
                    onclick={() => (channel = option)}
                ></button>
            {/each}
        </div>
    </div>
    <div class="plot relative w-full aspect-square rounded-[6px]" bind:clientWidth={size}>
        <canvas class="histogram absolute inset-0 w-full h-full" bind:this={histogramCanvas}></canvas>
        <svg
            class="absolute inset-0 w-full h-full"
            class:dragging
            bind:this={svg}
            viewBox="0 0 {size} {size}"
            role="application"
            aria-label="{LABELS[active]} curve"
            onpointerdown={handlePointerDown}
            onpointermove={handlePointerMove}
            onpointerup={endDrag}
            onpointercancel={endDrag}
            ondblclick={handleDoubleClick}
        >
            {#each GRID as g (g)}
                <line class="grid" x1={g * size} y1="0" x2={g * size} y2={size} />
                <line class="grid" x1="0" y1={g * size} x2={size} y2={g * size} />
            {/each}
            <line class="diagonal" x1="0" y1={size} x2={size} y2="0" />

            {#each drawOrder as id (id)}
                <path
                    class="curve"
                    class:active={id === active}
                    d={paths[id]}
                    stroke={CURVE_COLOURS[id]}
                />
            {/each}

            {#each curves[active] as point, i (i)}
                <circle
                    class="point"
                    cx={point.x * size}
                    cy={(1 - point.y) * size}
                    r="4"
                    stroke={CURVE_COLOURS[active]}
                />
            {/each}
        </svg>
    </div>
</div>

<style>
    .channel {
        width: 12px;
        height: 12px;
        border-radius: 50%;
        background: var(--swatch);
        outline: 1.5px solid transparent;
        outline-offset: 2px;
        cursor: pointer;
        position: relative;
        transition:
            outline-color 120ms ease,
            opacity 120ms ease;
    }
    .channel.selected {
        outline-color: var(--color1);
    }
    .channel:disabled {
        opacity: 0.25;
        cursor: default;
    }
    .channel.adjusted::after {
        content: "";
        position: absolute;
        left: 50%;
        bottom: -7px;
        width: 3px;
        height: 3px;
        margin-left: -1.5px;
        border-radius: 50%;
        background: var(--color2);
    }
    .plot {
        background: black;
    }
    .histogram {
        opacity: 0.22;
        border-radius: inherit;
    }
    svg {
        overflow: visible;
        cursor: crosshair;
        touch-action: none;
    }
    svg.dragging {
        cursor: grabbing;
    }
    .grid {
        stroke: rgba(255, 255, 255, 0.08);
        stroke-width: 1;
    }
    .diagonal {
        stroke: rgba(255, 255, 255, 0.12);
        stroke-width: 1;
        stroke-dasharray: 3 3;
    }
    .curve {
        fill: none;
        stroke-width: 1.25;
        opacity: 0.3;
    }
    .curve.active {
        stroke-width: 1.75;
        opacity: 1;
    }
    .point {
        fill: black;
        stroke-width: 1.5;
    }
</style>
