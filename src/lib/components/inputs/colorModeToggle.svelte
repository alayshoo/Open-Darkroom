<script lang="ts">
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

<button
	class="bg relative flex w-16 h-7 border-0 rounded-[10px] p-0"
	onclick={toggle}
	role="switch"
	aria-checked={isRgb}
	aria-label="Toggle between RGB and composite color mode"
>
	<svg width="64" height="28">
		<circle cx="17" cy="12" r="6" fill="#ff0000" style="mix-blend-mode: screen" />
		<circle cx="14" cy="17" r="6" fill="#00ff00" style="mix-blend-mode: screen" />
		<circle cx="20" cy="17" r="6" fill="#0000ff" style="mix-blend-mode: screen" />
		<circle cx="49" cy="14" r="6" fill="white" />
	</svg>
	<div
		class="toggle absolute w-8.75 h-7 rounded-[10px]"
		class:toggle-active={!isRgb}
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
		left: 34px;
		width: 30px;
	}
</style>