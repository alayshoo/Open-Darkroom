<!-- lib/components/settings/SettingsRow.svelte -->
<script lang="ts">
    /*
     * One line inside a SettingsGroup: a label on the left, and on the right
     * either a control the caller supplies as children or a read-only `value`.
     * Both forms exist because a settings panel is always a mix of the two.
     */
    import type { Snippet } from "svelte";

    let {
        label,
        description,
        value,
        children,
    }: {
        label: string;
        description?: string;
        value?: string;
        children?: Snippet;
    } = $props();
</script>

<div class="row flex items-center justify-between gap-5 px-3 py-1.5">
    <div class="text flex flex-col shrink-0">
        <span class="label">{label}</span>
        {#if description}
            <span class="description">{description}</span>
        {/if}
    </div>
    {#if children}
        <div class="control shrink-0">
            {@render children()}
        </div>
    {:else if value !== undefined}
        <!-- Free to shrink and wrap: some of what the GPU reports is a
             sentence, not a word. -->
        <span class="value min-w-0">{value}</span>
    {/if}
</div>

<style>
    /* The label/value pair reads like a slider's: the name quiet on --color2,
       the figure beside it a size up and brighter. */
    .label {
        font-family: "Figtree", sans-serif;
        font-size: 13px;
        font-weight: 400;
        line-height: 1.5;
        color: var(--color2);
    }

    .description {
        font-family: "Figtree", sans-serif;
        font-size: 11.5px;
        line-height: 1.35;
        color: var(--color3);
    }

    .value {
        font-family: "Figtree", sans-serif;
        font-size: 14px;
        font-weight: 500;
        line-height: 1.4;
        font-variant-numeric: tabular-nums;
        color: var(--color1s);
        text-align: right;
        /* Breaks a word only when it cannot fit a line of its own, so a
           comma-separated list still breaks at its commas. */
        overflow-wrap: break-word;
    }
</style>
