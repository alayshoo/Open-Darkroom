<!-- lib/components/library/ImageCell.svelte -->
<script lang="ts">
    import type { ImageRecord } from "$lib/library/catalogApi";
    import { library } from "$lib/library/library.svelte";

    let {
        image,
        selected,
        onselect,
        onopen,
        onmenu,
    }: {
        image: ImageRecord;
        selected: boolean;
        onselect: (e: MouseEvent) => void;
        onopen: () => void;
        onmenu: (e: MouseEvent) => void;
    } = $props();

    let src = $derived(library.thumbs.get(image.id));

    const withoutExtension = (name: string) => name.replace(/(.)\.[^.]+$/, "$1");

    $effect(() => {
        void library.requestThumbnail(image);
    });
</script>

<div
    class="cell flex flex-col"
    class:selected
    class:missing={image.missing}
    role="option"
    aria-selected={selected}
    aria-label={image.fileName}
    tabindex="-1"
    onclick={onselect}
    ondblclick={onopen}
    oncontextmenu={onmenu}
    onkeydown={(e) => e.key === "Enter" && onopen()}
>
    <div class="frame relative flex items-center justify-center">
        {#if src}
            <img data-thumb-id={image.id} {src} alt="" draggable="false" />
        {:else}
            <div class="placeholder" data-thumb-id={image.id}></div>
        {/if}
        {#if image.missing}
            <span class="missing-badge">{library.finding ? "Looking for it…" : "Missing"}</span>
        {/if}
    </div>
    <div class="caption flex items-baseline gap-1.5">
        {#if image.frame !== null}
            <span class="frame-no">{image.frame}</span>
        {/if}
        <span class="file flex-1" title={image.fileName}>{withoutExtension(image.fileName)}</span>
        {#if image.edited}
            <span class="edited" title="Edited" aria-label="Edited"></span>
        {/if}
    </div>
</div>

<style>
    .cell {
        gap: 6px;
        padding: 8px 8px 6px;
        border-radius: 12px;
        outline: none;
        cursor: default;
        transition: background 0.12s ease;
    }

    .cell:hover {
        background: rgba(255, 255, 255, 0.04);
    }

    .cell.selected {
        background: rgba(255, 255, 255, 0.09);
    }

    .frame {
        aspect-ratio: 1;
        width: 100%;
    }

    /* Laid over the square rather than in it, so a tall image is held to it. */
    img,
    .placeholder {
        position: absolute;
        inset: 0;
        margin: auto;
    }

    img {
        max-width: 100%;
        max-height: 100%;
        display: block;
        border-radius: 2px;
        box-shadow: 0 4px 14px rgba(0, 0, 0, 0.45);
    }

    .placeholder {
        width: 80%;
        height: 60%;
        border-radius: 3px;
        background: var(--bg4);
    }

    .missing img,
    .missing .placeholder {
        opacity: 0.35;
    }

    .missing-badge {
        position: absolute;
        font-size: 11px;
        padding: 2px 7px;
        border-radius: 6px;
        background: rgba(0, 0, 0, 0.7);
        color: #ffb4a8;
    }

    .edited {
        align-self: center;
        flex-shrink: 0;
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--color2);
    }

    .caption {
        min-width: 0;
        padding: 0 2px;
        font-size: 12px;
        line-height: 1.3;
    }

    .frame-no {
        color: var(--color1);
        font-weight: 600;
        font-variant-numeric: tabular-nums;
    }

    .file {
        color: var(--color3);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
