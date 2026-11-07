<!-- lib/components/editor/InfoPanel.svelte -->
<script lang="ts">
    import { editor } from "$lib/editor/editor.svelte";
    import { hasShootingInfo, metadataRows } from "$lib/types/imageMetadata";

    let rows = $derived(
        editor.metadata && editor.imagePayload
            ? metadataRows(editor.metadata, editor.imagePayload.fullWidth, editor.imagePayload.fullHeight)
            : [],
    );
</script>

<!-- No panel of its own, like the presets: the rows sit straight on the chrome. -->
<div class="info-panel flex flex-col h-full">
    <header class="flex items-center shrink-0 ml-2.5 mr-1 mt-2.5 h-7">
        <span class="title flex-1">Info</span>
    </header>
    <div class="rows flex flex-col flex-1 min-h-0 pt-2 pr-1 pb-6">
        {#if rows.length > 0}
            <dl class="m-0 px-2.5 flex flex-col gap-2.5">
                {#each rows as row (row.label)}
                    <div class="row flex flex-col">
                        <dt>{row.label}</dt>
                        <dd class="m-0">{row.value}</dd>
                    </div>
                {/each}
            </dl>
            {#if editor.metadata && !hasShootingInfo(editor.metadata)}
                <p class="note m-0 px-2.5 pt-4">This file does not say how it was shot.</p>
            {/if}
        {:else}
            <p class="note m-0 px-2.5 pt-2">Open an image to see how it was shot.</p>
        {/if}
    </div>
</div>

<style>
    .info-panel {
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.65);
    }

    .title {
        font-family: var(--font-ui);
        font-size: 18px;
        font-weight: 600;
        color: var(--color1);
    }

    .rows {
        overflow-y: auto;
        scrollbar-width: none;
    }

    .rows::-webkit-scrollbar {
        display: none;
    }

    dt {
        font-size: 12px;
        line-height: 1.3;
        color: var(--color3);
    }

    dd {
        font-size: 14px;
        line-height: 1.35;
        color: var(--color1);
        overflow-wrap: anywhere;
        user-select: text;
        -webkit-user-select: text;
    }

    .note {
        font-family: var(--font-ui);
        font-size: 13px;
        line-height: 1.45;
        color: var(--color3);
    }
</style>
