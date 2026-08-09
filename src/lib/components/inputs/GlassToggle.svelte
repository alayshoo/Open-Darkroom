<!-- lib/components/inputs/GlassToggle.svelte -->
<script lang="ts">
	/*
	 * The chrome behind every two-position toggle in the app: a flat black well
	 * with a glass thumb inset 3px inside it. Callers supply the icons as
	 * children — absolutely positioned against the track, which is the
	 * positioning context — plus the two stops the thumb travels between.
	 *
	 * Those stops are hand-tuned per toggle to land on that toggle's icons, so
	 * they are passed in rather than derived from the track width: every
	 * formula that could produce them would be overridden at all three call
	 * sites anyway.
	 */
	import type { Snippet } from "svelte";

	type Stop = { x: number; w: number };

	let {
		checked,
		label,
		width,
		thumb,
		onclick,
		children,
	}: {
		checked: boolean;
		label: string;
		width: number;
		thumb: { off: Stop; on: Stop };
		onclick?: () => void;
		children?: Snippet;
	} = $props();

	/*
	 * The caller owns the state. `checked` is only what the thumb and the
	 * screen reader report — this component never flips it, because the three
	 * toggles disagree about who mutates what (the color-mode toggle's
	 * `checked` is the negation of its own prop, and the mode toggle lets the
	 * page flip a value that drives the theme). Each keeps its own quirk.
	 */
	let stop = $derived(checked ? thumb.on : thumb.off);
</script>

<button
	class="bg relative flex h-7 border-0 rounded-[10px] p-0"
	style="--track-w: {width}px; --thumb-x: {stop.x}px; --thumb-w: {stop.w}px;"
	{onclick}
	role="switch"
	aria-checked={checked}
	aria-label={label}
>
	<div class="toggle glass absolute h-5.5 rounded-[7px]"></div>
	{@render children?.()}
</button>

<style>
	/* The well stays flat black — only the thumb is glass. The hairline and the
	   inner shadow are what make the 3px margin around the thumb read as a
	   recess rather than as padding. */
	.bg {
		width: var(--track-w);
		background: var(--bg1);
		box-shadow:
			inset 0 1px 2px rgba(0, 0, 0, 0.9),
			inset 0 0 0 1px rgba(255, 255, 255, 0.04);
		transition: transform 0.12s ease;
	}

	.bg:active {
		transform: scale(0.97);
	}

	/* Shares the app's frosted material (styles/glass.css); only the lighting is
	   pushed harder, since a 24px chip needs a brighter rim than a panel to read
	   as glass at all. No backdrop-filter: the well behind it is flat black, so
	   the blur bought nothing and cost a compositor pass per frame of travel —
	   the tint and the rim carry the material on their own. */
	.toggle {
		--panel-angle: 137deg;
		--panel-tint: rgba(255, 255, 255, 0.07);
		--rim-hi: 0.5;
		--rim-mid: 0.14;
		--rim-lo: 0.05;
		--rim-end: 0.12;

		top: 3px;
		left: var(--thumb-x);
		width: var(--thumb-w);
		box-sizing: border-box;
		backdrop-filter: none;
		-webkit-backdrop-filter: none;
		box-shadow:
			1.5px 1.8px 5px rgba(0, 0, 0, 0.55),
			inset -4px -4px 6px -4px rgba(12, 26, 40, 0.25);
		transition:
			left 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
			width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
			transform 0.12s ease;
		pointer-events: none;
	}

	/* Only the thumb reacts to hover, and only by growing — scaling from its
	   centre, so it swells into the 3px recess without ever reaching the well's
	   edge. The well itself holds its size. */
	.bg:hover .toggle {
		transform: scale(1.08);
	}

	/* glass.css ships its gloss at zero alpha for the big panels; a chip this
	   small wants the lit corner back. */
	.toggle::after {
		background: linear-gradient(
			var(--panel-angle),
			rgba(255, 255, 255, 0.3) 0%,
			rgba(255, 255, 255, 0.1) 33%,
			rgba(255, 255, 255, 0) 55%
		);
	}
</style>
