<script lang="ts">
	let {
		isDarkroom = $bindable(false),
		onModeToggle,
	}: {
		isDarkroom: boolean;
		onModeToggle?: () => void;
	} = $props();

</script>

<button
	class="bg relative flex w-13.25 h-7 border-0 rounded-[10px] p-0"
	onclick={onModeToggle}
	role="switch"
	aria-checked={isDarkroom}
	aria-label="Toggle between RGB and composite color mode"
>
	<div
		class="toggle absolute w-6.75 h-7 rounded-[10px]"
		class:toggle-active={isDarkroom}
	></div>
	<div class="icons flex w-13.25 h-7">
		<img src="/sun_icon.png" class="absolute size-4" style="top: 6px; left: 6px;" alt="Lighter-Room Mode">
		<img src="/lightbulb_icon.png" class="absolute size-5" style="top: 4px; right: 4px;" alt="Darkroom Mode">
	</div>
</button>

<style>
	.bg {
		background: var(--bg1);
	}

	.toggle {
		top: 0;
		left: 0;
		box-sizing: border-box;
		box-shadow: inset 0 0 3px 1px var(--color2);
		transition: left 0.2s cubic-bezier(0.2, 0.0, 0, 1.0), width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0);
		pointer-events: none;
	}

	.toggle-active {
		left: 26px;
	}
</style>