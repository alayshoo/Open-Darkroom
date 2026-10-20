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
		<svg width="40" height="40" viewBox="-6 -6 36 36">
			<path
				d="M8 4.5V16H19.5M4.5 8H16V19.5"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
</div>

<style>
	/* Same ink as the reset button, so it swings to red with the theme. */
	.crop-button :global(svg) {
		color: var(--color2);
		position: relative;
	}
</style>
