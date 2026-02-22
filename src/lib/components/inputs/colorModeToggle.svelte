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
	class="bg"
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
	<div class="toggle" class:toggle-active={!isRgb}></div>
</button>

<style>
	.bg {
		position: relative;
		display: flex;
		width: 64px;
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
		width: 35px;
		height: 28px;
		border-radius: 10px;
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