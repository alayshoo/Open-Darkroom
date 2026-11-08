<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let { onclick }: { onclick?: () => void } = $props();
</script>

<div class="back-button">
	<GlassButton label="Back to library" size={40} onclick={() => onclick?.()}>
		<!-- On the crop icon's padded view box, like the other tools. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<path
				d="M19 12H5.5M11 6.5L5.5 12L11 17.5"
				fill="none"
				stroke="currentColor"
				stroke-width="1.05"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
</div>

<style>
	.back-button :global(svg) {
		color: var(--chromeInk);
		position: relative;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}
</style>
