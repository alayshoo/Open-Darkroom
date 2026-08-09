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
     theme swap and the pager — this only reports the state. -->
<GlassToggle
	checked={isDarkroom}
	label="Toggle between lighter-room and darkroom mode"
	width={53}
	thumb={{ off: { x: 3, w: 24 }, on: { x: 26, w: 24 } }}
	onclick={onModeToggle}
>
	<img src="/sun_icon.png" class="absolute size-4" style="top: 6px; left: 7px;" alt="Lighter-Room Mode">
	<img src="/lightbulb_icon.png" class="absolute size-5" style="top: 4px; right: 5px;" alt="Darkroom Mode">
</GlassToggle>
