<script lang="ts">
	import GlassToggle from "./GlassToggle.svelte";

	let {
		isDarkroom = $bindable(false),
		onModeToggle,
	}: {
		isDarkroom: boolean;
		onModeToggle?: () => void;
	} = $props();
</script>

<!-- The page flips `isDarkroom` itself, since the same handler drives the
     theme swap and the pager — this only reports the state. Stops land on
     the circle centres at cx=15 and cx=38. -->
<GlassToggle
	checked={isDarkroom}
	label="Toggle between lighter-room and darkroom mode"
	width={53}
	thumb={{ off: { x: 3, w: 24 }, on: { x: 26, w: 24 } }}
	onclick={onModeToggle}
>
	<svg width="53" height="28" aria-hidden="true">
		<circle cx="15" cy="14" r="6" fill="white" />
		<circle cx="38" cy="14" r="6" fill="#FF0A0A" />
	</svg>
</GlassToggle>
