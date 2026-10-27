<!-- src/lib/components/outputs/PreviewDebugCard.svelte -->
<script lang="ts">
    import { onMount } from "svelte";
    import { previewDebug } from "$lib/gpu/previewDebug.svelte";
    import { validBox, type Box, type ChainRegion } from "$lib/utils/viewport";

    onMount(() => {
        previewDebug.enabled = true;
        return () => {
            previewDebug.enabled = false;
        };
    });

    let info = $derived(previewDebug.state);
    let counters = $derived(previewDebug.counters);
    let stream = $derived(previewDebug.stream);
    let input = $derived(previewDebug.input);

    // A held region in image pixels: its whole box, and the part fully rendered.
    const inImage = (box: Box, scale: number): Box => box.map((v) => v / scale) as Box;
    const rect = (b: Box) => ({ x: b[0], y: b[1], width: b[2] - b[0], height: b[3] - b[1] });

    // The mip levels a region reads, with how much of each: the sampler blends
    // the two around log2 of how far the region shrinks the source.
    function levelsOf(region: ChainRegion | null): Map<number, number> {
        const levels = new Map<number, number>();
        if (!info || !region) return levels;
        const shrink = info.source.width / info.fullWidth / region.scale;
        const lod = Math.min(Math.max(Math.log2(shrink), 0), info.source.mipLevels - 1);
        const lo = Math.floor(lod);
        const f = lod - lo;
        levels.set(lo, 1 - f);
        if (f > 1e-3) levels.set(lo + 1, f);
        return levels;
    }

    let sharpLevels = $derived(levelsOf(info?.detail ?? null));
    let backdropLevels = $derived(levelsOf(info?.backdrop ?? null));

    let levels = $derived.by(() => {
        if (!info) return [];
        const { width, height, mipLevels } = info.source;
        return Array.from({ length: mipLevels }, (_, i) => ({
            level: i,
            width: Math.max(1, width >> i),
            height: Math.max(1, height >> i),
            sharp: sharpLevels.get(i),
            backdrop: backdropLevels.get(i),
        }));
    });

    // The finest few levels, and the rest folded into one line unless one of
    // them is in use.
    let shown = $derived(levels.filter((l) => l.level < 6 || l.sharp !== undefined || l.backdrop !== undefined));
    let folded = $derived(levels.length - shown.length);

    let pad = $derived(info ? Math.max(info.fullWidth, info.fullHeight) * 0.04 : 0);
    let viewBox = $derived(
        info ? `${-pad} ${-pad} ${info.fullWidth + 2 * pad} ${info.fullHeight + 2 * pad}` : "0 0 1 1",
    );
    let stroke = $derived(info ? Math.max(info.fullWidth, info.fullHeight) / 300 : 1);

    const percent = (w: number | undefined) => (w === undefined ? "" : `${Math.round(w * 100)}%`);
    const size = (w: number, h: number) => `${w}×${h}`;

    // Ticks once a second while the card is up, so the input line ages.
    let now = $state(performance.now());
    $effect(() => {
        const timer = setInterval(() => (now = performance.now()), 1000);
        return () => clearInterval(timer);
    });
</script>

<div class="panel glass rounded-[12px] p-2.5">
    <h2>Preview · regions and levels</h2>

    {#if info}
        <svg class="map" {viewBox} preserveAspectRatio="xMidYMid meet" aria-label="What the preview holds">
            <rect class="image" x="0" y="0" width={info.fullWidth} height={info.fullHeight} />
            {#if info.backdrop}
                {@const b = rect(inImage(validBox(info.backdrop), info.backdrop.scale))}
                <rect class="backdrop" class:shown={info.backdropShown} {...b} stroke-width={stroke} />
            {/if}
            {#if info.detail}
                {@const whole = rect(inImage(info.detail.box, info.detail.scale))}
                {@const valid = rect(inImage(validBox(info.detail), info.detail.scale))}
                <rect class="apron" {...whole} stroke-width={stroke} stroke-dasharray="{stroke * 3} {stroke * 2}" />
                <rect class="sharp" {...valid} stroke-width={stroke} />
            {/if}
            {#if info.visible}
                <rect class="visible" {...rect(info.visible)} stroke-width={stroke * 1.5} />
            {/if}
        </svg>

        <ul class="legend">
            <li><span class="swatch visible"></span><span class="name">On screen</span>
                <span class="outside">{info.scale >= 1 ? `${info.scale.toFixed(2)}×` : `${Math.round(info.scale * 100)}%`}</span></li>
            <li><span class="swatch sharp"></span><span class="name">Sharp region</span>
                <span class="outside">{info.detail ? `${Math.round(info.detail.scale * 100)}% · apron ${info.detail.apron}` : "none"}</span></li>
            <li><span class="swatch backdrop" class:shown={info.backdropShown}></span><span class="name">Backdrop</span>
                <span class="outside">{info.backdrop ? `${Math.round(info.backdrop.scale * 100)}%${info.backdropShown ? " · shown" : ""}` : "none"}</span></li>
        </ul>

        <ul class="levels">
            {#each shown as l (l.level)}
                <li class:used={l.sharp !== undefined || l.backdrop !== undefined}>
                    <span class="name">L{l.level}</span>
                    <span class="dims">{size(l.width, l.height)}</span>
                    {#if l.sharp !== undefined}<span class="chip sharp">sharp {percent(l.sharp)}</span>{/if}
                    {#if l.backdrop !== undefined}<span class="chip backdrop">backdrop {percent(l.backdrop)}</span>{/if}
                </li>
            {/each}
            {#if folded > 0}
                <li class="fold"><span class="name">+{folded} smaller, unused</span></li>
            {/if}
        </ul>

        <p class="note">
            Source {size(info.source.width, info.source.height)}
            {#if stream && !stream.done}
                · streaming {Math.round((stream.rows / stream.total) * 100)}%
            {:else if info.source.width < info.fullWidth}
                · preview only
            {:else}
                · full resolution
            {/if}
            <br />
            Sharp renders {counters.detailRenders} · last {counters.lastDetailMs.toFixed(1)} ms
            <br />
            Backdrop renders {counters.backdropRenders} · put off {counters.deferred}
            <br />
            Input {input ? `${input.type}, ${input.detail}, ${((now - input.at) / 1000).toFixed(0)} s ago` : "none yet"}
        </p>
    {:else}
        <p class="note">Open an image to see what the preview holds.</p>
    {/if}
</div>

<style>
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

    .map {
        display: block;
        width: 100%;
        max-height: 220px;
        border-radius: 6px;
        background: var(--bg1);
    }

    .map .image {
        fill: rgba(255, 255, 255, 0.06);
    }

    .map .backdrop {
        fill: none;
        stroke: rgba(240, 180, 90, 0.55);
    }

    .map .backdrop.shown {
        fill: rgba(240, 180, 90, 0.12);
    }

    .map .apron {
        fill: none;
        stroke: rgba(110, 210, 200, 0.6);
    }

    .map .sharp {
        fill: rgba(110, 210, 200, 0.22);
        stroke: rgba(110, 210, 200, 0.9);
    }

    .map .visible {
        fill: none;
        stroke: rgba(255, 255, 255, 0.9);
    }

    ul {
        margin: 0.55rem 0 0;
        padding: 0.45rem 0 0;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        list-style: none;
    }

    li {
        display: flex;
        align-items: center;
        gap: 0.45rem;
        padding: 0.12rem 0;
    }

    .swatch {
        width: 9px;
        height: 9px;
        border-radius: 2px;
        box-sizing: border-box;
    }

    .swatch.visible {
        border: 1.5px solid rgba(255, 255, 255, 0.9);
    }

    .swatch.sharp {
        background: rgba(110, 210, 200, 0.6);
    }

    .swatch.backdrop {
        border: 1.5px solid rgba(240, 180, 90, 0.7);
    }

    .swatch.backdrop.shown {
        background: rgba(240, 180, 90, 0.4);
    }

    .name {
        color: rgba(255, 255, 255, 0.78);
    }

    .outside {
        margin-left: auto;
        font-variant-numeric: tabular-nums;
        color: rgba(255, 255, 255, 0.45);
    }

    .levels li {
        color: rgba(255, 255, 255, 0.4);
    }

    .levels .name {
        min-width: 2.2em;
        color: rgba(255, 255, 255, 0.4);
    }

    .levels li.used .name,
    .levels li.used .dims {
        color: rgba(255, 255, 255, 0.85);
    }

    .dims {
        font-variant-numeric: tabular-nums;
    }

    .chip {
        margin-left: auto;
        padding: 0 0.35rem;
        border-radius: 3px;
        font-variant-numeric: tabular-nums;
    }

    .chip + .chip {
        margin-left: 0;
    }

    .chip.sharp {
        background: rgba(110, 210, 200, 0.22);
        color: rgba(160, 235, 228, 0.95);
    }

    .chip.backdrop {
        background: rgba(240, 180, 90, 0.18);
        color: rgba(250, 210, 150, 0.95);
    }

    .note {
        margin: 0.5rem 0 0;
        padding-top: 0.4rem;
        border-top: 1px solid rgba(255, 255, 255, 0.1);
        color: rgba(255, 255, 255, 0.38);
        line-height: 1.4;
    }
</style>
