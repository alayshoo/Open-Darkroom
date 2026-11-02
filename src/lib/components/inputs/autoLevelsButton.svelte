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
