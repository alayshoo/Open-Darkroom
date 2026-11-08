<!-- lib/components/library/FilmCanister.svelte -->
<script lang="ts">
    // A 35mm canister with its leader out, in a stock's colours: the body in
    // the first, and the label band around it in the second.
    let {
        colour = null,
        colour2 = null,
        size = 20,
    }: { colour?: string | null; colour2?: string | null; size?: number } = $props();

    let band = $derived(colour2 ?? (colour ? `color-mix(in srgb, ${colour} 65%, black)` : null));
</script>

<svg class="canister" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    {#if colour}
        <rect x="3.5" y="6" width="10" height="14" rx="2" fill={colour} />
        <rect x="3.5" y="10.5" width="10" height="5" fill={band} />
    {/if}
    <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round">
        <rect x="3.5" y="6" width="10" height="14" rx="2" />
        <path d="M6.5 6V3.8h4V6" />
        <path d="M13.5 9.5h5l1.5 1.5v5.5h-6.5" />
        <path d="M15.6 12h0.01M18 12h0.01" stroke-width="1.8" />
    </g>
</svg>

<style>
    .canister {
        flex-shrink: 0;
    }
</style>
