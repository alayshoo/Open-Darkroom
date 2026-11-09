<!-- lib/components/editor/ControlsPage.svelte -->
<script lang="ts">
    import type { Snippet } from "svelte";

    let { children }: { children: Snippet } = $props();
</script>

<!-- One page of a controls pager, scrolling on its own under its fades. -->
<div class="controls-page">
    {@render children()}
    <div class="flex h-7.5"></div>
</div>

<style>
    .controls-page {
        min-width: 100%;
        max-width: 100%;
        overflow-y: auto;
        scrollbar-width: none;
        scroll-snap-align: start;
        /* Top and bottom fades via mask - no overlays, so inner shadow on side-panel stays visible */
        mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            black 20px,
            black calc(100% - 24px),
            transparent 100%
        );
        -webkit-mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            black 20px,
            black calc(100% - 24px),
            transparent 100%
        );
    }
    .controls-page::-webkit-scrollbar {
        display: none;
    }
</style>
