<!-- lib/components/settings/AddRow.svelte -->
<script lang="ts">
    // The last row of a settings list, adding another to it.
    let { label, onclick }: { label: string; onclick: () => void } = $props();
</script>

<button class="add flex items-center" {onclick}>
    <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 5v14M5 12h14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    </svg>
    {label}
</button>

<style>
    .add {
        gap: 10px;
        width: 100%;
        height: 40px;
        padding: 0 12px;
        border: 0;
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 13px;
        text-align: left;
        cursor: default;
    }

    .add:hover {
        background: color-mix(in srgb, var(--glassRim) 4%, transparent);
        color: var(--color1);
    }
</style>
