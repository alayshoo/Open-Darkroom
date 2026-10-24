<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		visible = false,
		straightening = $bindable(false),
		canReset = false,
		onturn,
		onreset,
	}: {
		visible?: boolean;
		straightening?: boolean;
		// Whether the crop differs from the original image at all.
		canReset?: boolean;
		onturn?: (clockwise: boolean) => void;
		onreset?: () => void;
	} = $props();
</script>

<!-- Stays mounted and fades, like the reset button, so the column never jumps. -->
<div class="crop-tools flex flex-col items-center gap-1" class:shown={visible} aria-hidden={!visible}>
	<GlassButton size={34} label="Rotate left" onclick={() => onturn?.(false)}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<rect x="4.5" y="11" width="10" height="8.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" />
			<path
				d="M19.5 14.5A7.5 7.5 0 0 0 12 7M14.1 4.9 12 7 14.1 9.1"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
	<GlassButton size={34} label="Rotate right" onclick={() => onturn?.(true)}>
		<svg width="28" height="28" viewBox="0 0 24 24" style="transform: scaleX(-1);">
			<rect x="4.5" y="11" width="10" height="8.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" />
			<path
				d="M19.5 14.5A7.5 7.5 0 0 0 12 7M14.1 4.9 12 7 14.1 9.1"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
	<GlassButton
		size={34}
		label="Straighten: draw along a horizon"
		active={straightening}
		onclick={() => (straightening = !straightening)}
	>
		<!-- A ruler laid at a slant: draw along something that should be level. -->
		<svg width="28" height="28" viewBox="0 0 24 24">
			<g transform="rotate(-24 12 12)" fill="none" stroke="currentColor" stroke-linecap="round">
				<rect x="2.5" y="9.5" width="19" height="5" rx="1" stroke-width="1.2" />
				<path d="M6 9.5V11.5M9 9.5V11M12 9.5V12M15 9.5V11M18 9.5V11.5" stroke-width="1" />
			</g>
		</svg>
	</GlassButton>
	<!-- The same arrow as the adjustments' reset button. -->
	<GlassButton size={34} label="Reset crop" disabled={!canReset} onclick={() => onreset?.()}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<path
				d="M12.7 7.56A4.5 4.5 0 1 1 7.57 12.78"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
			/>
			<path d="M9.15 7.5 12 5.35 12 9.65Z" fill="currentColor" />
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
		margin: auto;
	}
</style>
