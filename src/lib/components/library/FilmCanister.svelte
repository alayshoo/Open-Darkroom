<!-- lib/components/library/FilmCanister.svelte -->
<script lang="ts">
    // A 35mm canister with its leader out, in a stock's colours: the body in
    // the first, and its foot in the second when there is one.
    let {
        colour = null,
        colour2 = null,
        size = 20,
    }: { colour?: string | null; colour2?: string | null; size?: number } = $props();
</script>

<svg class="canister" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    {#if colour}
        <rect x="3.5" y="6" width="10" height="14" rx="2" fill={colour} />
        {#if colour2}
            <path d="M3.5 16h10v2a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2z" fill={colour2} />
        {/if}
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
