<script lang="ts">
	import GlassButton from "./GlassButton.svelte";
	import { ASPECT_PRESETS, aspectLabel, type AspectLock, type AspectPresetId } from "$lib/types/cropAspect";

	let {
		visible = false,
		straightening = $bindable(false),
		aspectMenuOpen = $bindable(false),
		aspect,
		aspectRatio = null,
		canReset = false,
		onturn,
		onflip,
		onaspect,
		oncustom,
		onreset,
	}: {
		visible?: boolean;
		straightening?: boolean;
		aspectMenuOpen?: boolean;
		aspect: AspectLock;
		// The lock's width over height, or null when free.
		aspectRatio?: number | null;
		// Whether the crop differs from the original image at all.
		canReset?: boolean;
		onturn?: (clockwise: boolean) => void;
		onflip?: (horizontal: boolean) => void;
		onaspect?: (lock: AspectLock) => void;
		// Asks for a custom ratio to be entered.
		oncustom?: () => void;
		onreset?: () => void;
	} = $props();

	let aspectEl: HTMLDivElement | undefined = $state();

	let presetLabel = $derived(aspectLabel(aspect));
	let canSwap = $derived(aspectRatio !== null && aspectRatio !== 1);

	// The icon's box takes the lock's shape, within a 14-unit square.
	let iconBox = $derived.by(() => {
		const r = aspectRatio ?? 4 / 3;
		const w = r >= 1 ? 14 : 14 * r;
		const h = r >= 1 ? 14 / r : 14;
		return { x: 12 - w / 2, y: 12 - h / 2, w, h };
	});

	$effect(() => {
		if (!visible) aspectMenuOpen = false;
	});

	$effect(() => {
		if (!aspectMenuOpen) return;
		function closeOutside(e: PointerEvent) {
			if (!aspectEl?.contains(e.target as Node)) aspectMenuOpen = false;
		}
		document.addEventListener("pointerdown", closeOutside, true);
		return () => document.removeEventListener("pointerdown", closeOutside, true);
	});

	function choose(preset: AspectPresetId) {
		aspectMenuOpen = false;
		if (preset === "custom") oncustom?.();
		else onaspect?.({ ...aspect, preset });
	}
</script>

<!-- Stays mounted and fades, like the reset button, so the column never jumps. -->
<div class="crop-tools flex flex-col items-center gap-1" class:shown={visible} aria-hidden={!visible}>
	<GlassButton size={34} label="Rotate left" onclick={() => onturn?.(false)}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<rect x="4.5" y="11" width="10" height="8.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" />
			<path
				d="M19.5 14.5A7.5 7.5 0 0 0 12 7M14.1 4.9 12 7 14.1 9.1"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
	<GlassButton size={34} label="Rotate right" onclick={() => onturn?.(true)}>
		<svg width="28" height="28" viewBox="0 0 24 24" style="transform: scaleX(-1);">
			<rect x="4.5" y="11" width="10" height="8.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.2" />
			<path
				d="M19.5 14.5A7.5 7.5 0 0 0 12 7M14.1 4.9 12 7 14.1 9.1"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
				stroke-linejoin="round"
			/>
		</svg>
	</GlassButton>
	<GlassButton size={34} label="Flip horizontally" onclick={() => onflip?.(true)}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<g fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round">
				<path d="M12 4.5V19.5" stroke-dasharray="1.6 1.6" stroke-linecap="round" />
				<path d="M10 7V17H4.5Z" />
				<path d="M14 7V17H19.5Z" fill="currentColor" fill-opacity="0.35" />
			</g>
		</svg>
	</GlassButton>
	<GlassButton size={34} label="Flip vertically" onclick={() => onflip?.(false)}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<g transform="rotate(90 12 12)" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round">
				<path d="M12 4.5V19.5" stroke-dasharray="1.6 1.6" stroke-linecap="round" />
				<path d="M10 7V17H4.5Z" />
				<path d="M14 7V17H19.5Z" fill="currentColor" fill-opacity="0.35" />
			</g>
		</svg>
	</GlassButton>
	<GlassButton
		size={34}
		label="Straighten: draw along a horizon"
		active={straightening}
		onclick={() => (straightening = !straightening)}
	>
		<!-- A ruler laid at a slant: draw along something that should be level. -->
		<svg width="28" height="28" viewBox="0 0 24 24">
			<g transform="rotate(-24 12 12)" fill="none" stroke="currentColor" stroke-linecap="round">
				<rect x="2.5" y="9.5" width="19" height="5" rx="1" stroke-width="1.2" />
				<path d="M6 9.5V11.5M9 9.5V11M12 9.5V12M15 9.5V11M18 9.5V11.5" stroke-width="1" />
			</g>
		</svg>
	</GlassButton>
	<!-- The same arrow as the adjustments' reset button. -->
	<GlassButton size={34} label="Reset crop" disabled={!canReset} onclick={() => onreset?.()}>
		<svg width="28" height="28" viewBox="0 0 24 24">
			<path
				d="M12.7 7.56A4.5 4.5 0 1 1 7.57 12.78"
				fill="none"
				stroke="currentColor"
				stroke-width="1.2"
				stroke-linecap="round"
			/>
			<path d="M9.15 7.5 12 5.35 12 9.65Z" fill="currentColor" />
		</svg>
	</GlassButton>
	<div class="aspect relative" bind:this={aspectEl}>
		<GlassButton
			size={34}
			label="Aspect ratio: {presetLabel}"
			active={aspectMenuOpen}
			onclick={() => (aspectMenuOpen = !aspectMenuOpen)}
		>
			<svg width="28" height="28" viewBox="0 0 24 24">
				<rect
					x={iconBox.x}
					y={iconBox.y}
					width={iconBox.w}
					height={iconBox.h}
					rx="1.2"
					fill="none"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-dasharray={aspectRatio === null ? "2 1.6" : undefined}
				/>
			</svg>
		</GlassButton>
		{#if aspectMenuOpen}
			<div class="aspect-menu absolute flex flex-col rounded-[8px] p-1.5" role="menu" aria-label="Aspect ratio">
				{#each ASPECT_PRESETS as preset (preset.id)}
					<button
						class="aspect-option border-0 rounded-[5px] text-left"
						class:active={preset.id === aspect.preset}
						role="menuitemradio"
						aria-checked={preset.id === aspect.preset}
						onclick={() => choose(preset.id)}
					>
						{preset.label}
						{#if preset.id === "custom" && aspect.preset === "custom"}
							<span class="custom-value">{presetLabel}</span>
						{/if}
					</button>
				{/each}
			</div>
		{/if}
	</div>
	<GlassButton
		size={34}
		label={aspect.portrait ? "Make landscape" : "Make portrait"}
		disabled={!canSwap}
		onclick={() => onaspect?.({ ...aspect, portrait: !aspect.portrait })}
	>
		<!-- A landscape frame and a portrait one, swapped by the arrow between. -->
		<svg width="28" height="28" viewBox="0 0 24 24">
			<g fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
				<rect x="3.5" y="12" width="10" height="7" rx="1" />
				<rect x="14" y="3.5" width="6.5" height="9.5" rx="1" />
				<path d="M5.5 9.5A5 5 0 0 1 10.5 4.5M8.6 3 10.5 4.5 8.6 6" />
			</g>
		</svg>
	</GlassButton>
</div>

<style>
	.crop-tools {
		opacity: 0;
		visibility: hidden;
		transition:
			opacity 0.25s ease,
			visibility 0s linear 0.25s;
	}

	.crop-tools.shown {
		opacity: 1;
		visibility: visible;
		transition:
			opacity 0.25s ease,
			visibility 0s linear 0s;
	}

	.aspect-menu {
		left: calc(100% + 8px);
		top: 0;
		min-width: 96px;
		z-index: 100;
		background: var(--bg3);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
		animation: aspectMenuIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
	}

	@keyframes aspectMenuIn {
		from {
			opacity: 0;
			transform: translateX(-8px) scale(0.97);
		}
		to {
			opacity: 1;
			transform: translateX(0) scale(1);
		}
	}

	.aspect-option {
		padding: 4px 10px;
		white-space: nowrap;
		font: 13px var(--font-ui);
		font-variant-numeric: tabular-nums;
		background: transparent;
		color: var(--color2);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.custom-value {
		margin-left: 6px;
		opacity: 0.7;
	}

	.aspect-option:hover {
		background: var(--bg4);
	}

	.aspect-option.active {
		background: var(--color1);
		color: var(--bg1);
		font-weight: 600;
	}

	.crop-tools :global(svg) {
		color: var(--chromeInk);
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
		position: relative;
		margin: auto;
	}
</style>
