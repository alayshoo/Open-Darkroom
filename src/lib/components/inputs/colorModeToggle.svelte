<script lang="ts">
	import GlassToggle from "./GlassToggle.svelte";

	let {
		isRgb = $bindable(true),
		onToggle,
	}: {
		isRgb: boolean;
		onToggle?: (isBw: boolean) => void;
	} = $props();

	function toggle() {
		isRgb = !isRgb;
		onToggle?.(!isRgb);
	}
</script>

<!-- Reads inverted: the thumb travels when RGB is *off*. The active stop is
     narrower because the RGB triad is 18px wide and the lone white circle
     only 12px. -->
<GlassToggle
	checked={!isRgb}
	label="Toggle between RGB and composite color mode"
	width={54}
	thumb={{ off: { x: 3, w: 24 }, on: { x: 27, w: 24 } }}
	onclick={toggle}
>
	<svg width="64" height="28">
		<circle cx="15" cy="12" r="4" fill="#ff0000" style="mix-blend-mode: screen" />
		<circle cx="13" cy="16" r="4" fill="#00ff00" style="mix-blend-mode: screen" />
		<circle cx="17" cy="16" r="4" fill="#0000ff" style="mix-blend-mode: screen" />
		<circle cx="39" cy="14" r="6" fill="white" />
	</svg>
</GlassToggle>
