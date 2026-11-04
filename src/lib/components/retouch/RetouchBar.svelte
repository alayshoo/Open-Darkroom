<!-- lib/components/retouch/RetouchBar.svelte -->
<script lang="ts">
	import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
	import GlassButton from "$lib/components/inputs/GlassButton.svelte";
	import RevertButton from "$lib/components/inputs/revertButton.svelte";
	import { MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS, retouchTool } from "$lib/retouch/retouchTool.svelte";

	let {
		open = false,
		strokes = 0,
		onclear,
	}: {
		open?: boolean;
		// How many strokes the image holds.
		strokes?: number;
		onclear?: () => void;
	} = $props();

	// Held while the bar slides shut, so its title does not blank mid-way.
	let shown = $state<"heal" | "clone">("heal");
	$effect(() => {
		if (retouchTool.mode) shown = retouchTool.mode;
	});
</script>

<div class="retouch-bar" class:open inert={!open}>
	<div class="band"></div>
	<div class="row flex items-center" role="group" aria-label="{shown === 'heal' ? 'Heal' : 'Clone'} brush">
		<span class="title shrink-0">{shown === "heal" ? "Heal" : "Clone"}</span>
		<div class="slider">
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
		</div>
		<div class="slider">
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
		</div>
		<div class="slider">
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
		</div>
		<div class="buttons flex shrink-0 items-center gap-2">
			<div class="well">
				<GlassButton
					label="Auto source"
					disabled={!retouchTool.hasSource}
					onclick={() => retouchTool.resetSource()}
				>
					<span class="glyph absolute">A</span>
				</GlassButton>
			</div>
			<RevertButton visible={strokes > 0} label="Clear all" onrevert={() => onclear?.()} />
		</div>
	</div>
</div>

<style>
	/* Across the foot of the window, over the image and under the toolbar. */
	.retouch-bar {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: 9;
		pointer-events: none;
		opacity: 0;
		visibility: hidden;
		transform: translateY(24px);
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.65);
		transition:
			opacity 0.25s ease,
			transform 0.25s cubic-bezier(0.4, 0, 1, 1),
			visibility 0s linear 0.25s;
	}

	/* Waits for the side bar to finish leaving before it rises. */
	.retouch-bar.open {
		opacity: 1;
		visibility: visible;
		transform: none;
		transition:
			opacity 0.3s ease 0.35s,
			transform 0.3s cubic-bezier(0.2, 0, 0, 1) 0.35s,
			visibility 0s linear 0.35s;
	}

	/* The left band's ramp turned on its side: a dense band and a faint tail. */
	.band,
	.band::after {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
	}

	.band {
		height: 88px;
		background: linear-gradient(to bottom, var(--scrim-ramp));
		opacity: var(--scrimStrength);
	}

	.band::after {
		content: "";
		height: 230px;
		background: linear-gradient(to bottom, var(--scrim-ramp));
		opacity: 0.35;
	}

	.row {
		position: relative;
		gap: 28px;
		height: 52px;
		padding: 0 28px 4px 76px;
		pointer-events: auto;
	}

	.title {
		width: 56px;
		font-family: var(--font-ui);
		font-size: 18px;
		font-weight: 600;
		color: var(--color1);
	}

	.slider {
		flex: 1;
		min-width: 120px;
		max-width: 220px;
	}

	/* The same flat black well as the auto levels button. */
	.well {
		display: flex;
		border-radius: 10px;
		background: var(--bg1);
		box-shadow:
			inset 0 1px 2px rgba(0, 0, 0, 0.9),
			inset 0 0 0 1px color-mix(in srgb, var(--glassRim) 4%, transparent);
	}

	.glyph {
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: white;
		font-size: 14px;
		font-weight: 600;
		line-height: 1;
		pointer-events: none;
		text-shadow: none;
	}
</style>
