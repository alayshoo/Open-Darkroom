<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		visible = true,
		onrevert,
	}: {
		visible?: boolean;
		onrevert?: () => void;
	} = $props();
</script>

<!-- Stays mounted and fades, so the row's layout never moves under it. -->
<div class="revert-slot" class:shown={visible} aria-hidden={!visible}>
	<GlassButton label="Reset all adjustments" onclick={() => onrevert?.()}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<!-- The arc starts a few degrees past the top so its cap tucks in
			     behind the head rather than poking out of it. -->
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
	/* Visibility steps at the far end of the fade so the hidden button stops
	   taking clicks and leaves the tab order. */
	.revert-slot {
		opacity: 0;
		visibility: hidden;
		transition:
			opacity 0.5s ease,
			visibility 0s linear 0.5s;
	}

	.revert-slot.shown {
		opacity: 1;
		visibility: visible;
		transition:
			opacity 0.5s ease,
			visibility 0s linear 0s;
	}

	/* Same ink as the Export label, so it swings to red with the theme. */
	.revert-slot :global(svg) {
		color: var(--color2);
	}
</style>
