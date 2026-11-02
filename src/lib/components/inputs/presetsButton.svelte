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

<div class="presets-button">
	<GlassButton
		label={active ? "Hide presets" : "Presets"}
		size={40}
		{active}
		{disabled}
		onclick={() => ontoggle?.()}
	>
		<!-- A stack of frames, drawn on the crop icon's padded view box and
		     within the same 15-unit square. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<g
				fill="none"
				stroke="currentColor"
				stroke-width="1.05"
				stroke-linecap="round"
				stroke-linejoin="round"
			>
				<rect x="4.5" y="9.5" width="10" height="10" rx="1.5" />
				<path d="M7 7H15.5A1.5 1.5 0 0 1 17 8.5V17" />
				<path d="M9.5 4.5H18A1.5 1.5 0 0 1 19.5 6V14.5" />
			</g>
		</svg>
	</GlassButton>
</div>

<style>
	.presets-button :global(svg) {
		color: var(--chromeInk);
		position: relative;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}
</style>
