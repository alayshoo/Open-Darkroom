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
	class="bg"
	onclick={onModeToggle}
	role="switch"
	aria-checked={isDarkroom}
	aria-label="Toggle between RGB and composite color mode"
>
	<div class="toggle" class:toggle-active={isDarkroom}></div>
	<div class="icons">
		<img src="/sun_icon.png" style="position: absolute; width: 16px; height: 16px; top: 6px; left: 6px;" alt="Lighter-Room Mode">
		<img src="/lightbulb_icon.png" style="position: absolute; width: 20px; height: 20px; top: 4px; right: 4px;" alt="Darkroom Mode">
	</div>
</button>

<style>
	.bg {
		position: relative;
		display: flex;
		width: 53px;
		height: 28px;
		border: none;
		border-radius: 10px;
		background: var(--bg1);
		padding: 0;
	}

	.toggle {
		position: absolute;
		top: 0;
		left: 0;
		width: 27px;
		height: 28px;
		border-radius: 10px;
		box-sizing: border-box;
		box-shadow: inset 0 0 3px 1px var(--color2);
		transition: left 0.2s cubic-bezier(0.2, 0.0, 0, 1.0), width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0);
		pointer-events: none;
	}

	.toggle-active {
		left: 26px;
	}

	.icons {
		display: flex;
		width: 53px;
		height: 28px;
	}
</style>