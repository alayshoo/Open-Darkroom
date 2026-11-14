<!-- lib/components/library/FilmCanister.svelte -->
<script lang="ts">
    // A 35mm canister with its leader out, in a stock's colours: the body in
    // the first, and its foot in the second, running into the third when there is one.
    let {
        colour = null,
        colour2 = null,
        colour3 = null,
        size = 20,
    }: { colour?: string | null; colour2?: string | null; colour3?: string | null; size?: number } = $props();

    const id = $props.id();
    let foot = $derived(colour2 && colour3 ? `url(#${id}-foot)` : colour2);
</script>

<svg class="canister" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
    {#if colour2 && colour3}
        <defs>
            <linearGradient id="{id}-foot" x1="0" x2="1" y1="0" y2="0">
                <stop offset="0.2" stop-color={colour2} />
                <stop offset="0.8" stop-color={colour3} />
            </linearGradient>
        </defs>
    {/if}
    {#if colour}
        <rect x="3.5" y="6" width="10" height="14" rx="2" fill={colour} />
        {#if foot}
            <path d="M3.5 13.5h10v4.5a2 2 0 0 1-2 2h-6a2 2 0 0 1-2-2z" fill={foot} />
        {/if}
        <rect x="3.5" y="6" width="10" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="0.5" stroke-opacity="0.6" />
    {:else}
        <rect x="3.5" y="6" width="10" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="1.1" />
    {/if}
    <g fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round" stroke-linecap="butt">
        <path d="M6.5 6V4h4v2" />
        <path d="M13.5 9.5h5l1.5 1.5v5.5h-6.5" />
    </g>
    <path d="M15.6 12h0.01M18 12h0.01" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
</svg>

<style>
    .canister {
        flex-shrink: 0;
    }
</style>
