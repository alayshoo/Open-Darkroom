<script lang="ts">
	import GlassButton from "./GlassButton.svelte";
	import SingleValSliderGroup from "./values/SingleValSliderGroup.svelte";
	import { MAX_BRUSH_RADIUS, MIN_BRUSH_RADIUS, retouchTool } from "$lib/retouch/retouchTool.svelte";
	import type { RetouchMode } from "$lib/types/retouch";

	let {
		disabled = false,
		strokes = 0,
		ontoggle,
		onclear,
	}: {
		disabled?: boolean;
		// How many strokes the image holds.
		strokes?: number;
		ontoggle?: (mode: RetouchMode) => void;
		onclear?: () => void;
	} = $props();

	const pickKey = typeof navigator !== "undefined" && /Mac/.test(navigator.platform) ? "⌘" : "Ctrl";

	let mode = $derived(retouchTool.mode);
</script>

<div class="retouch-tools relative flex flex-col items-center gap-1">
	<GlassButton label="Heal" size={40} active={mode === "heal"} {disabled} onclick={() => ontoggle?.("heal")}>
		<!-- A plaster laid at a slant. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<g transform="rotate(-45 12 12)" fill="none" stroke="currentColor" stroke-width="1.05" stroke-linejoin="round">
				<rect x="3" y="8.5" width="18" height="7" rx="3.5" />
				<path d="M9 8.5V15.5M15 8.5V15.5" />
			</g>
			<g transform="rotate(-45 12 12)" fill="currentColor">
				<circle cx="11" cy="11" r="0.6" />
				<circle cx="13" cy="11" r="0.6" />
				<circle cx="11" cy="13" r="0.6" />
				<circle cx="13" cy="13" r="0.6" />
			</g>
		</svg>
	</GlassButton>
	<GlassButton label="Clone" size={40} active={mode === "clone"} {disabled} onclick={() => ontoggle?.("clone")}>
		<!-- A rubber stamp. -->
		<svg width="40" height="40" viewBox="-3 -3 30 30">
			<g fill="none" stroke="currentColor" stroke-width="1.05" stroke-linecap="round" stroke-linejoin="round">
				<path d="M10 12.5V10.2A3 3 0 1 1 14 10.2V12.5" />
				<path d="M6 15.5A1.5 1.5 0 0 1 7.5 14H16.5A1.5 1.5 0 0 1 18 15.5V17H6Z" />
				<path d="M6.5 19.5H17.5" />
			</g>
		</svg>
	</GlassButton>

	{#if mode && !disabled}
		<div class="retouch-options absolute flex flex-col rounded-[10px]" role="group" aria-label="{mode === 'heal' ? 'Heal' : 'Clone'} brush">
			<span class="title">{mode === "heal" ? "Heal" : "Clone"}</span>
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
				{pickKey}-click picks a source · Alt-drag sizes and softens · Space pans
			</p>
		</div>
	{/if}
</div>

<style>
	.retouch-tools :global(svg) {
		color: var(--chromeInk);
		position: relative;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}

	.retouch-options {
		left: calc(100% + 8px);
		top: 0;
		width: 220px;
		padding: 10px 12px 10px 14px;
		gap: 6px;
		z-index: 100;
		background: var(--bg3);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
		animation: optionsIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
	}

	@keyframes optionsIn {
		from {
			opacity: 0;
			transform: translateX(-8px) scale(0.97);
		}
	}

	.title {
		font: 600 13px var(--font-ui);
		color: var(--color1);
		margin-bottom: 4px;
	}

	.actions {
		margin-top: 6px;
	}

	.action {
		flex: 1;
		padding: 4px 8px;
		border: 0;
		border-radius: 5px;
		font: 12px var(--font-ui);
		background: var(--bg4);
		color: var(--color2);
		cursor: pointer;
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
		margin: 4px 0 0;
		font: 11px var(--font-ui);
		line-height: 1.35;
		color: var(--color2);
		opacity: 0.7;
	}
</style>
