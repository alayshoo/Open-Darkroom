<script lang="ts">
	import GlassButton from "./GlassButton.svelte";

	let {
		disabled = false,
		onauto,
	}: {
		disabled?: boolean;
		onauto?: () => void;
	} = $props();
</script>

<div class="auto-levels">
	<GlassButton label="Auto black and white points" {disabled} onclick={() => onauto?.()}>
		<span class="glyph absolute">A</span>
	</GlassButton>
</div>

<style>
	/* The same flat black well the toggles beside it sit in. */
	.auto-levels {
		display: flex;
		border-radius: 10px;
		background: var(--bg1);
		box-shadow:
			inset 0 1px 2px rgba(0, 0, 0, 0.9),
			inset 0 0 0 1px color-mix(in srgb, var(--glassRim) 4%, transparent);
	}

	.glyph {
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: white;
		font-size: 14px;
		font-weight: 600;
		line-height: 1;
		pointer-events: none;
	}
</style>
