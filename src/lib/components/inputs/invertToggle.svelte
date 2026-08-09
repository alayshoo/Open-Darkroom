<script lang="ts">
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

<button
	class="bg relative flex w-15 h-7 border-0 rounded-[10px] p-0"
	onclick={toggle}
	role="switch"
	aria-checked={isInverted}
	aria-label="Toggle image inversion"
>
	<svg width="60" height="28">
		<circle cx="15" cy="14" r="6" fill="white" />
		<circle cx="45" cy="14" r="6" fill="none" stroke="white" stroke-width="2" />
	</svg>
	<div
		class="toggle absolute w-7.5 h-7 rounded-[10px]"
		class:toggle-active={isInverted}
	></div>
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
		left: 30px;
		width: 30px;
	}
</style>
