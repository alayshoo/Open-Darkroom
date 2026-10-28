<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		active = false,
		ontoggle,
	}: {
		active?: boolean;
		ontoggle?: () => void;
	} = $props();
</script>

<div class="crop-button">
	<GlassButton
		label={active ? "Apply crop" : "Crop"}
		size={40}
		{active}
		onclick={() => ontoggle?.()}
	>
		<!-- The view box is padded out past the 24-unit drawing, so the icon
		     sits small and centred inside the larger chip. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<path
				d="M8 4.5V16H19.5M4.5 8H16V19.5"
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
	.crop-button :global(svg) {
		color: var(--chromeInk);
		position: relative;
		/* Matches the title bar's glyphs, which sit on the same scrim. */
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}
</style>
