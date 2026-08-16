<!-- lib/components/inputs/GlassButton.svelte -->
<script lang="ts">
	/*
	 * GlassToggle's thumb on its own: the same glass material, sized square,
	 * with no well behind it and nowhere to travel. Callers supply the icon as
	 * children, positioned against the chip the way the toggles position theirs.
	 */
	import type { Snippet } from "svelte";

	let {
		label,
		size = 28,
		onclick,
		children,
	}: {
		label: string;
		size?: number;
		onclick?: () => void;
		children?: Snippet;
	} = $props();
</script>

<button
	class="bg relative flex border-0 rounded-[10px] p-0"
	style="--size: {size}px;"
	{onclick}
	aria-label={label}
	title={label}
>
	<div class="face glass absolute rounded-[7px]"></div>
	{@render children?.()}
</button>

<style>
	.bg {
		width: var(--size);
		height: var(--size);
		background: transparent;
		transition: transform 0.12s ease;
	}

	.bg:active {
		transform: scale(0.92);
	}

	/* Same material and lighting as the toggle thumb — see GlassToggle. It does
	   not grow on hover the way the thumb does; it simply fades in behind the
	   icon. The press is the only thing that moves. */
	.face {
		--panel-angle: 137deg;
		--panel-tint: color-mix(in srgb, var(--glassRim) 7%, transparent);
		--rim-hi: 0.5;
		--rim-mid: 0.14;
		--rim-lo: 0.05;
		--rim-end: 0.12;

		/* 24×22 inside the 28px box — the toggle thumb's exact dimensions, so
		   the chip reads as one of the toggles sitting at the end of the row.
		   The box itself stays square and keeps the hit area. */
		inset: 3px 2px;
		box-sizing: border-box;
		backdrop-filter: none;
		-webkit-backdrop-filter: none;
		box-shadow:
			1.5px 1.8px 5px rgba(0, 0, 0, 0.55),
			inset -4px -4px 6px -4px rgba(12, 26, 40, 0.25);
		opacity: 0;
		transition: opacity 0.15s ease;
		pointer-events: none;
	}

	.bg:hover .face,
	.bg:active .face {
		opacity: 1;
	}

	.face::after {
		background: linear-gradient(
			var(--panel-angle),
			color-mix(in srgb, var(--glassRim) 30%, transparent) 0%,
			color-mix(in srgb, var(--glassRim) 10%, transparent) 33%,
			color-mix(in srgb, var(--glassRim) 0%, transparent) 55%
		);
	}
</style>
