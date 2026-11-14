<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		active = false,
		disabled = false,
		ontoggle,
	}: {
		active?: boolean;
		disabled?: boolean;
		ontoggle?: () => void;
	} = $props();
</script>

<div class="info-button">
	<GlassButton
		label={active ? "Hide image info" : "Image info"}
		size={40}
		{active}
		{disabled}
		onclick={() => ontoggle?.()}
	>
		<!-- On the crop icon's padded view box, sized to the 1:1 label below it. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<g fill="none" stroke="currentColor" stroke-width="1.05" stroke-linecap="round">
				<circle cx="12" cy="12" r="4.15" />
				<path d="M12 11.75V13.9" />
			</g>
			<circle cx="12" cy="9.95" r="0.55" fill="currentColor" />
		</svg>
	</GlassButton>
</div>

<style>
	.info-button :global(svg) {
		color: var(--chromeInk);
		position: relative;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}
</style>
