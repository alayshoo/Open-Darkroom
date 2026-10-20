<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		visible = false,
		straightening = $bindable(false),
		onturn,
	}: {
		visible?: boolean;
		straightening?: boolean;
		onturn?: (clockwise: boolean) => void;
	} = $props();
</script>

<!-- Stays mounted and fades, like the reset button, so the column never jumps. -->
<div class="crop-tools flex flex-col items-center gap-1.5" class:shown={visible} aria-hidden={!visible}>
	<GlassButton label="Rotate left" onclick={() => onturn?.(false)}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<path
				d="M8.2 9.3A5 5 0 1 1 7.5 14"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
			/>
			<path d="M6.2 6.6 9.9 7.6 7.4 10.4Z" fill="currentColor" />
		</svg>
	</GlassButton>
	<GlassButton label="Rotate right" onclick={() => onturn?.(true)}>
		<svg width="28" height="28" viewBox="0 0 24 24" style="transform: scaleX(-1);">
			<path
				d="M8.2 9.3A5 5 0 1 1 7.5 14"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
			/>
			<path d="M6.2 6.6 9.9 7.6 7.4 10.4Z" fill="currentColor" />
		</svg>
	</GlassButton>
	<GlassButton
		label="Straighten: draw along a horizon"
		active={straightening}
		onclick={() => (straightening = !straightening)}
	>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<path
				d="M5 16.5H19M6 13.5 18 9"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
			/>
			<path
				d="M14 16.5A4.5 4.5 0 0 0 13.4 10.6"
				fill="none"
				stroke="currentColor"
				stroke-width="1"
				stroke-linecap="round"
				stroke-dasharray="1 1.6"
			/>
		</svg>
	</GlassButton>
</div>

<style>
	.crop-tools {
		opacity: 0;
		visibility: hidden;
		transition:
			opacity 0.25s ease,
			visibility 0s linear 0.25s;
	}

	.crop-tools.shown {
		opacity: 1;
		visibility: visible;
		transition:
			opacity 0.25s ease,
			visibility 0s linear 0s;
	}

	/* Same ink as the reset button, so it swings to red with the theme. */
	.crop-tools :global(svg) {
		color: var(--color2);
		position: relative;
	}
</style>
