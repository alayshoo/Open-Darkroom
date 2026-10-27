<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		fitted = true,
		disabled = false,
		ontoggle,
	}: {
		// Whether the image is fitted to its slot, so the button offers 1:1.
		fitted?: boolean;
		disabled?: boolean;
		ontoggle?: () => void;
	} = $props();
</script>

<div class="zoom-button">
	<GlassButton
		label={fitted ? "Zoom to 1:1" : "Fit to view"}
		size={40}
		{disabled}
		onclick={() => ontoggle?.()}
	>
		<span class="face-label absolute inset-0 flex items-center justify-center">
			{fitted ? "1:1" : "Fit"}
		</span>
	</GlassButton>
</div>

<style>
	/* Same ink as the crop button. */
	.face-label {
		color: var(--color2);
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.02em;
		pointer-events: none;
		text-shadow:
			0 1px 3px rgba(0, 0, 0, 0.75),
			0 0 10px rgba(0, 0, 0, 0.45);
	}
</style>
