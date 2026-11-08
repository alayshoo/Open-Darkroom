<!-- lib/components/library/RollCard.svelte -->
<script lang="ts">
    import type { RollRecord } from "$lib/library/catalogApi";
    import { library } from "$lib/library/library.svelte";
    import { rollSummary } from "$lib/types/rollInfo";

    let { roll, onmenu }: { roll: RollRecord; onmenu: (e: MouseEvent) => void } = $props();

    const STRIP = 4;
    let frames = $derived(library.framesOf(roll.id));
    let strip = $derived(frames.slice(0, STRIP));
    let summary = $derived(rollSummary(roll.info));

    $effect(() => {
        for (const frame of strip) void library.requestThumbnail(frame);
    });
</script>

<button class="roll-card flex flex-col" onclick={() => library.show({ kind: "roll", id: roll.id })} oncontextmenu={onmenu}>
    <!-- A strip of the first frames, the way a contact sheet starts. -->
    <div class="strip flex">
        {#each { length: STRIP } as _, i (i)}
            {@const frame = strip[i]}
            <div class="slot flex items-center justify-center">
                {#if frame && library.thumbs[frame.id]}
                    <img src={library.thumbs[frame.id]} alt="" draggable="false" />
                {/if}
            </div>
        {/each}
    </div>
    <div class="meta flex flex-col">
        <span class="name">{roll.name}</span>
        <span class="summary">{summary || "No film information yet"}</span>
        <span class="count">{frames.length} frame{frames.length === 1 ? "" : "s"}</span>
    </div>
</button>

<style>
    .roll-card {
        gap: 10px;
        padding: 10px;
        border: 0;
        border-radius: 14px;
        background: var(--bg3);
        color: inherit;
        text-align: left;
        cursor: default;
        transition: background 0.12s ease;
    }

    .roll-card:hover {
        background: var(--bg4);
    }

    /* Film base between the frames, with sprocket holes along both edges. */
    .strip {
        gap: 3px;
        padding: 9px 4px;
        border-radius: 4px;
        background:
            radial-gradient(circle, rgba(255, 255, 255, 0.13) 1.4px, transparent 1.8px) 0 2px / 9px 100% repeat-x,
            radial-gradient(circle, rgba(255, 255, 255, 0.13) 1.4px, transparent 1.8px) 0 calc(100% - 2px) / 9px 100% repeat-x,
            #0b0b0b;
        background-size: 9px 6px, 9px 6px, auto;
        background-position: 0 1px, 0 calc(100% - 1px), 0 0;
    }

    .slot {
        flex: 1;
        aspect-ratio: 3 / 2;
        background: #151515;
        overflow: hidden;
    }

    .slot img {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }

    .meta {
        gap: 2px;
        padding: 0 2px;
    }

    .name {
        font-family: var(--font-ui);
        font-size: 14px;
        font-weight: 600;
        color: var(--color1);
    }

    .summary {
        font-size: 12px;
        color: var(--color2);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .count {
        font-size: 12px;
        color: var(--color3);
    }
</style>
