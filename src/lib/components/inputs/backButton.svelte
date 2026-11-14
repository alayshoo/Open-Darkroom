<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let { disabled = false, onclick }: { disabled?: boolean; onclick?: () => void } = $props();
</script>

<div class="back-button">
	<GlassButton label="Back to library" size={40} {disabled} onclick={() => onclick?.()}>
		<!-- On the crop icon's padded view box, nudged towards its tip so it reads as centred. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<path
				d="M13.5 6.5L9.45 10.55Q8 12 9.45 13.45L13.5 17.5"
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
