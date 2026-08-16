<!-- lib/components/settings/SettingsGroup.svelte -->
<script lang="ts">
    /* A titled block of rows. Sections are built out of these and nothing
       else, so every panel keeps the same rhythm. */
    import type { Snippet } from "svelte";

    let {
        title,
        note,
        children,
    }: {
        title?: string;
        note?: string;
        children: Snippet;
    } = $props();
</script>

<section class="group flex flex-col">
    {#if title}
        <h3 class="group-title m-0 mb-1">{title}</h3>
    {/if}
    <div class="rows flex flex-col rounded-[10px] overflow-hidden">
        {@render children()}
    </div>
    {#if note}
        <p class="group-note m-0 mt-1.5">{note}</p>
    {/if}
</section>

<style>
    /* A step under the panel's own heading, on the side bar's section-title
       weight and colour. Indented onto the rows' own text column, so it clears
       the corner the table rounds off underneath it. */
    .group-title {
        font-family: "Figtree", sans-serif;
        font-size: 15px;
        font-weight: 600;
        color: var(--color1);
        padding-left: 12px;
    }

    /* The rows share one surface, split by hairlines rather than by gaps. The
       divider lives here rather than on the row so it never depends on which
       component happened to emit the sibling. */
    .rows {
        background: var(--bg4);
    }

    .rows > :global(* + *) {
        border-top: 1px solid var(--bg5);
    }

    .group-note {
        font-family: "Figtree", sans-serif;
        font-size: 11.5px;
        color: var(--color3);
        padding-left: 12px;
    }
</style>
