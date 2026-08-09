<script lang="ts">
	import GlassToggle from "./GlassToggle.svelte";

	let {
		isInverted = $bindable(false),
		onToggle,
	}: {
		isInverted: boolean;
		onToggle?: (isInverted: boolean) => void;
	} = $props();

	function toggle() {
		isInverted = !isInverted;
		onToggle?.(isInverted);
	}
</script>

<!-- Stops land on the circle centres at cx=15 and cx=45. -->
<GlassToggle
	checked={isInverted}
	label="Toggle image inversion"
	width={54}
	thumb={{ off: { x: 3, w: 24 }, on: { x: 27, w: 24 } }}
	onclick={toggle}
>
	<svg width="60" height="28">
		<circle cx="15" cy="14" r="6" fill="white" />
		<circle cx="39" cy="14" r="6" fill="none" stroke="white" stroke-width="2" />
	</svg>
</GlassToggle>
