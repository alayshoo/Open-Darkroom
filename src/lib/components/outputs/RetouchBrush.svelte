<!-- src/lib/components/outputs/RetouchBrush.svelte -->
<script lang="ts">
    import type { Point } from "$lib/utils/viewport";

    // Everything in CSS pixels of the window.
    let {
        cursor,
        radius,
        feather,
        source = null,
        picked = null,
        strokes = [],
    }: {
        // Where the brush is drawn, or null while it is off the image.
        cursor: Point | null;
        radius: number;
        // 0 to 1.
        feather: number;
        // Where the pixels under the brush come from.
        source?: Point | null;
        // A source picked and not yet used.
        picked?: Point | null;
        // Strokes being painted, or waiting to be applied.
        strokes?: { points: Point[]; radius: number }[];
    } = $props();

    const path = (points: Point[]) =>
        points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`).join(" ") +
        (points.length === 1 ? " l0.01 0" : "");

    let inner = $derived(radius * (1 - feather));
</script>

<svg class="retouch-brush" aria-hidden="true">
    {#each strokes as stroke}
        <path class="stroke" d={path(stroke.points)} stroke-width={2 * stroke.radius} />
    {/each}
    {#if source && cursor}
        <circle class="ring dark" cx={source[0]} cy={source[1]} r={radius} />
        <circle class="ring source" cx={source[0]} cy={source[1]} r={radius} />
    {/if}
    {#if picked}
        <g transform="translate({picked[0]} {picked[1]})">
            <path class="ring dark" d="M-8 0H8M0 -8V8" />
            <path class="ring" d="M-8 0H8M0 -8V8" />
            <circle class="ring dark" r={radius} />
            <circle class="ring source" r={radius} />
        </g>
    {/if}
    {#if cursor}
        <circle class="ring dark" cx={cursor[0]} cy={cursor[1]} r={radius} />
        <circle class="ring" cx={cursor[0]} cy={cursor[1]} r={radius} />
        {#if inner > 1 && inner < radius - 1}
            <circle class="ring inner" cx={cursor[0]} cy={cursor[1]} r={inner} />
        {/if}
    {/if}
</svg>

<style>
    .retouch-brush {
        position: fixed;
        inset: 0;
        width: 100vw;
        height: 100vh;
        pointer-events: none;
        overflow: visible;
    }

    .stroke {
        fill: none;
        stroke: rgba(255, 255, 255, 0.35);
        stroke-linecap: round;
        stroke-linejoin: round;
    }

    .ring {
        fill: none;
        stroke: rgba(255, 255, 255, 0.9);
        stroke-width: 1;
    }

    /* A dark edge under the light one, so the ring reads on any tone. */
    .ring.dark {
        stroke: rgba(0, 0, 0, 0.6);
        stroke-width: 3;
    }

    .ring.inner {
        stroke: rgba(255, 255, 255, 0.5);
        stroke-dasharray: 3 3;
    }

    .ring.source {
        stroke-dasharray: 4 3;
    }
</style>
