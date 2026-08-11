<!-- src/lib/components/outputs/DebugStatsOverlay.svelte -->
<script lang="ts">
    import {
        debugStats,
        IDLE_AFTER_MS,
        type PassTiming,
    } from "$lib/gpu/debugStats.svelte";

    // Average each pass across the frames that carried it, in the order the
    // recorder first reported them — a frame that skipped a pass simply
    // contributes nothing to it rather than pulling its mean down.
    function meanPasses(all: PassTiming[]): PassTiming[] {
        const totals = new Map<string, { total: number; count: number }>();
        for (const pass of all) {
            const entry = totals.get(pass.label) ?? { total: 0, count: 0 };
            entry.total += pass.ms;
            entry.count += 1;
            totals.set(pass.label, entry);
        }
        return [...totals].map(([label, { total, count }]) => ({
            label,
            ms: total / count,
        }));
    }

    // Frames to average the printed figures over. Per-frame numbers flicker
    // faster than they can be read; the graph keeps the detail.
    const WINDOW = 30;

    const GRAPH_WIDTH = 168;
    const GRAPH_HEIGHT = 34;
    // The 60fps budget, drawn as a reference line.
    const BUDGET_MS = 1000 / 60;

    let graph: HTMLCanvasElement | undefined = $state();
    let idle = $state(false);

    // A frame only arrives when something asks for one, so an untouched slider
    // leaves the last reading standing. Say so rather than implying 0 fps.
    $effect(() => {
        const timer = setInterval(() => {
            idle = performance.now() - debugStats.lastRecordedAt > IDLE_AFTER_MS;
        }, 250);
        return () => clearInterval(timer);
    });

    let summary = $derived.by(() => {
        debugStats.version;
        const recent = debugStats.samples.slice(-WINDOW);
        if (recent.length === 0) return null;

        const mean = (pick: (s: (typeof recent)[number]) => number) =>
            recent.reduce((total, s) => total + pick(s), 0) / recent.length;

        // Intervals are only meaningful once there is a previous frame to
        // measure from, so the first sample of a run is left out.
        const intervals = recent.filter((s) => s.intervalMs > 0);
        const intervalMs = intervals.length
            ? intervals.reduce((total, s) => total + s.intervalMs, 0) / intervals.length
            : 0;

        // The real GPU cost of the chain, and the number to watch when tuning a
        // pass. Frames the timer had to skip carry no passes and would drag the
        // mean down, so they are left out rather than counted as zero.
        const timedFrames = recent.filter((s) => s.passes.length > 0);
        const passes = meanPasses(timedFrames.flatMap((s) => s.passes));
        const passesMs = passes.reduce((total, p) => total + p.ms, 0);

        return {
            fps: intervalMs > 0 ? 1000 / intervalMs : 0,
            intervalMs,
            acquireMs: mean((s) => s.acquireMs),
            recordMs: mean((s) => s.recordMs),
            turnaroundMs: mean((s) => s.turnaroundMs),
            passesMs,
            passes,
        };
    });

    // Histogram updates run on their own cadence, so they average separately.
    let histogram = $derived.by(() => {
        debugStats.histogramVersion;
        const recent = debugStats.histogramSamples.slice(-WINDOW);
        if (recent.length === 0) return null;

        const recordMs =
            recent.reduce((total, s) => total + s.recordMs, 0) / recent.length;

        const passes = meanPasses(
            recent.filter((s) => s.passes.length > 0).flatMap((s) => s.passes),
        );
        const gpuMs = passes.reduce((total, p) => total + p.ms, 0);

        return { gpuMs, recordMs, passes };
    });

    // Redraw the sparkline whenever a frame lands. A canvas keeps this off the
    // DOM — 180 elements rebuilt at frame rate would cost more than it measures.
    $effect(() => {
        debugStats.version;

        const ctx = graph?.getContext("2d");
        if (!ctx || !graph) return;

        const dpr = window.devicePixelRatio || 1;
        graph.width = GRAPH_WIDTH * dpr;
        graph.height = GRAPH_HEIGHT * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, GRAPH_WIDTH, GRAPH_HEIGHT);

        const samples = debugStats.samples;
        if (samples.length === 0) return;

        // Scale to the worst frame on screen, never tighter than the budget, so
        // a comfortable run does not look alarming.
        const peak = Math.max(BUDGET_MS, ...samples.map((s) => s.turnaroundMs));
        const barWidth = GRAPH_WIDTH / samples.length;
        const y = (ms: number) => GRAPH_HEIGHT - (ms / peak) * GRAPH_HEIGHT;

        ctx.strokeStyle = "rgba(255,255,255,0.22)";
        ctx.setLineDash([2, 3]);
        ctx.beginPath();
        ctx.moveTo(0, y(BUDGET_MS));
        ctx.lineTo(GRAPH_WIDTH, y(BUDGET_MS));
        ctx.stroke();
        ctx.setLineDash([]);

        for (const [i, sample] of samples.entries()) {
            const height = GRAPH_HEIGHT - y(sample.turnaroundMs);
            ctx.fillStyle =
                sample.turnaroundMs > BUDGET_MS ? "rgb(240,150,90)" : "rgb(120,200,150)";
            ctx.fillRect(i * barWidth, y(sample.turnaroundMs), Math.max(barWidth - 0.5, 0.5), height);
        }
    });

    function ms(value: number) {
        return `${value.toFixed(1)} ms`;
    }
</script>

<div class="debug-stats">
    <div class="headline">
        <span class="fps" class:idle>
            {idle || !summary ? "idle" : `${summary.fps.toFixed(0)} fps`}
        </span>
        <span class="total">{summary ? ms(summary.intervalMs) : "—"}</span>
    </div>

    <canvas
        bind:this={graph}
        style="width: {GRAPH_WIDTH}px; height: {GRAPH_HEIGHT}px;"
    ></canvas>

    {#if summary}
        <dl class="rows">
            <dt>acquire</dt>
            <dd class:warn={summary.acquireMs > 2}>{ms(summary.acquireMs)}</dd>
            <dt>record</dt>
            <dd>{ms(summary.recordMs)}</dd>
            <dt>turnaround</dt>
            <dd>{ms(summary.turnaroundMs)}</dd>
        </dl>

        {#if summary.passes.length > 0}
            <dl class="rows passes">
                <dt class="total">gpu passes</dt>
                <dd class="total">{ms(summary.passesMs)}</dd>
                {#each summary.passes as pass (pass.label)}
                    <dt class="child">{pass.label}</dt>
                    <dd>{ms(pass.ms)}</dd>
                {/each}
            </dl>
        {:else if !debugStats.supportsPassTimings}
            <p class="note">no timestamp-query on this adapter</p>
        {/if}

        {#if !debugStats.histogramEnabled}
            <p class="note">histogram off (F2)</p>
        {:else if histogram}
            <!-- Two totals rather than one: the GPU passes and the CPU that
                 records them do not overlap in time, so summing them would
                 describe no real duration. -->
            <dl class="rows passes">
                <dt class="total">histogram gpu</dt>
                <dd class="total">{ms(histogram.gpuMs)}</dd>
                {#each histogram.passes as pass (pass.label)}
                    <dt class="child">{pass.label}</dt>
                    <dd>{ms(pass.ms)}</dd>
                {/each}
                <dt class="total">histogram cpu</dt>
                <dd class="total">{ms(histogram.recordMs)}</dd>
            </dl>
        {/if}
    {:else}
        <p class="note">move a slider to sample</p>
    {/if}
</div>

<style>
    .debug-stats {
        position: absolute;
        top: 0.5rem;
        left: 0.5rem;
        z-index: 20;
        padding: 0.5rem 0.6rem;
        border-radius: 8px;
        font-family: ui-monospace, "Cascadia Mono", "Consolas", monospace;
        font-size: 10.5px;
        line-height: 1.45;
        color: rgba(255, 255, 255, 0.82);
        background: rgba(18, 18, 20, 0.72);
        border: 1px solid rgba(255, 255, 255, 0.1);
        backdrop-filter: blur(12px);
        pointer-events: none;
        user-select: none;
    }

    .headline {
        display: flex;
        justify-content: space-between;
        gap: 0.75rem;
        margin-bottom: 0.35rem;
        font-size: 12px;
    }

    .fps {
        font-weight: 600;
        color: rgb(120, 200, 150);
    }

    .fps.idle {
        color: rgba(255, 255, 255, 0.4);
    }

    .total {
        color: rgba(255, 255, 255, 0.6);
    }

    canvas {
        display: block;
        border-radius: 3px;
        background: rgba(255, 255, 255, 0.04);
    }

    .rows {
        display: grid;
        grid-template-columns: 1fr auto;
        gap: 0 0.75rem;
        margin: 0.4rem 0 0;
    }

    .passes {
        margin-top: 0.3rem;
        padding-top: 0.3rem;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        color: rgba(255, 255, 255, 0.62);
    }

    dt {
        color: rgba(255, 255, 255, 0.55);
    }

    dd {
        margin: 0;
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    dd.warn {
        color: rgb(240, 150, 90);
    }

    /* The sum of the passes is the chain's real GPU cost; the individual passes
       under it are the breakdown, so they read as subordinate. */
    dt.total,
    dd.total {
        color: rgba(255, 255, 255, 0.82);
    }

    dt.child {
        padding-left: 0.6rem;
        color: rgba(255, 255, 255, 0.45);
    }

    .note {
        margin: 0.4rem 0 0;
        color: rgba(255, 255, 255, 0.45);
    }
</style>
