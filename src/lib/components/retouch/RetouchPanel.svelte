<!-- lib/components/retouch/RetouchPanel.svelte -->
<script lang="ts">
	import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
	import { MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS, retouchTool } from "$lib/retouch/retouchTool.svelte";

	let {
		strokes = 0,
		onclear,
	}: {
		// How many strokes the image holds.
		strokes?: number;
		onclear?: () => void;
	} = $props();

	const pickKey = typeof navigator !== "undefined" && /Mac/.test(navigator.platform) ? "⌘" : "Ctrl";

	// Held while the column slides shut, so its title does not blank mid-way.
	let shown = $state<"heal" | "clone">("heal");
	$effect(() => {
		if (retouchTool.mode) shown = retouchTool.mode;
	});
</script>

<div class="retouch-panel flex flex-col h-full" role="group" aria-label="{shown === 'heal' ? 'Heal' : 'Clone'} brush">
	<header class="flex items-center shrink-0 ml-2.5 mr-1 mt-2.5 h-7">
		<span class="title flex-1">{shown === "heal" ? "Heal" : "Clone"}</span>
	</header>
	<div class="controls flex flex-col gap-1.5 ml-2.5 mr-2 mt-4">
		<SingleValSliderGroup
			bind:value={retouchTool.radius}
			defaultValue={12}
			name="Size"
			unit="px"
			min={MIN_BRUSH_RADIUS}
			max={150}
			allowOverflow
			hardMin={MIN_BRUSH_RADIUS}
			hardMax={MAX_BRUSH_RADIUS}
			decimalPlaces={0}
			dragStep={1}
			sliderStep={1}
			keyboardStep={1}
		/>
		<SingleValSliderGroup
			bind:value={retouchTool.feather}
			defaultValue={50}
			name="Feather"
			unit="%"
			min={0}
			max={100}
			decimalPlaces={0}
			dragStep={1}
			sliderStep={1}
			keyboardStep={1}
		/>
		<SingleValSliderGroup
			bind:value={retouchTool.opacity}
			defaultValue={100}
			name="Opacity"
			unit="%"
			min={1}
			max={100}
			decimalPlaces={0}
			dragStep={1}
			sliderStep={1}
			keyboardStep={1}
		/>
		<div class="actions flex flex-row gap-1.5">
			<button class="action" disabled={!retouchTool.hasSource} onclick={() => retouchTool.resetSource()}>
				{retouchTool.hasSource ? "Auto source" : "Source: auto"}
			</button>
			<button class="action" disabled={strokes === 0} onclick={() => onclear?.()}>Clear all</button>
		</div>
		<p class="hint">
			{pickKey}-click picks a source<br />
			Alt-drag sizes, and softens upwards<br />
			Space pans
		</p>
	</div>
</div>

<style>
	/* Straight on the chrome, like the presets list beside it. */
	.retouch-panel {
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.65);
	}

	.title {
		font-family: var(--font-ui);
		font-size: 18px;
		font-weight: 600;
		color: var(--color1);
	}

	.actions {
		margin-top: 10px;
	}

	.action {
		flex: 1;
		padding: 5px 8px;
		border: 0;
		border-radius: 6px;
		font: 12px var(--font-ui);
		background: var(--bg4);
		color: var(--color2);
		cursor: pointer;
		text-shadow: none;
		transition: background 0.15s ease, opacity 0.15s ease;
	}

	.action:hover:not(:disabled) {
		background: var(--color1);
		color: var(--bg1);
	}

	.action:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.hint {
		margin: 14px 0 0;
		font: 12px var(--font-ui);
		line-height: 1.6;
		color: var(--color3);
	}
</style>
