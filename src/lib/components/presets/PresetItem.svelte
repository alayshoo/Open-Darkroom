<!-- lib/components/presets/PresetItem.svelte -->
<script lang="ts">
	import GlassButton from "$lib/components/inputs/GlassButton.svelte";
	import type { Preset } from "$lib/presets/presetDocument";

	let {
		preset,
		canApply = true,
		onapply,
		onhover,
		onedit,
		onexport,
		ondelete,
	}: {
		preset: Preset;
		canApply?: boolean;
		onapply?: () => void;
		onhover?: (hovering: boolean) => void;
		onedit?: () => void;
		onexport?: () => void;
		ondelete?: () => void;
	} = $props();

	// Deleting takes a second press, so one stray click cannot lose a preset.
	const DELETE_ARM_MS = 3000;
	let deleteArmed = $state(false);
	let armTimer: ReturnType<typeof setTimeout> | null = null;

	function disarm() {
		deleteArmed = false;
		if (armTimer) clearTimeout(armTimer);
		armTimer = null;
	}

	function handleDelete() {
		if (deleteArmed) {
			disarm();
			ondelete?.();
			return;
		}
		deleteArmed = true;
		armTimer = setTimeout(disarm, DELETE_ARM_MS);
	}

	$effect(() => () => disarm());
</script>

<div
	class="item relative flex items-center"
	onpointerenter={() => onhover?.(true)}
	onpointerleave={() => {
		onhover?.(false);
		disarm();
	}}
	role="listitem"
>
	<button
		class="apply relative flex flex-1 items-center min-w-0 h-8 border-0 p-0"
		disabled={!canApply}
		title={canApply ? `Apply ${preset.name}` : preset.name}
		onclick={() => onapply?.()}
	>
		<div class="face glass absolute"></div>
		<span class="name relative truncate">{preset.name}</span>
	</button>
	<div class="actions absolute flex items-center">
		<GlassButton size={26} label="Modify preset" onclick={() => onedit?.()}>
			<svg width="26" height="26" viewBox="0 0 24 24">
				<path
					d="M14.5 7.5l2 2M8 16l.6-2.6 6.9-6.9a1.4 1.4 0 0 1 2 2l-6.9 6.9z"
					fill="none"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</GlassButton>
		<GlassButton size={26} label="Export preset" onclick={() => onexport?.()}>
			<svg width="26" height="26" viewBox="0 0 24 24">
				<path
					d="M12 13.5V6M9.2 8.6 12 5.8l2.8 2.8M7.5 12.5v4.5h9v-4.5"
					fill="none"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</GlassButton>
		<div class="delete" class:armed={deleteArmed}>
			<GlassButton
				size={26}
				label={deleteArmed ? "Click again to delete" : "Delete preset"}
				active={deleteArmed}
				onclick={handleDelete}
			>
				<svg width="26" height="26" viewBox="0 0 24 24">
					<path
						d="M7.5 8.5h9M10.5 8.5V7h3v1.5M8.7 8.5l.6 8.5h5.4l.6-8.5"
						fill="none"
						stroke="currentColor"
						stroke-width="1.2"
						stroke-linecap="round"
						stroke-linejoin="round"
					/>
				</svg>
			</GlassButton>
		</div>
	</div>
</div>

<style>
	.apply {
		border-radius: 10px;
		background: transparent;
		color: var(--color2);
		font-family: var(--font-ui);
		font-size: 13px;
		text-align: left;
		cursor: auto;
		transition:
			transform 0.12s ease,
			color 0.15s ease;
	}

	.apply:active {
		transform: scale(0.97);
	}

	.apply:disabled {
		opacity: 0.5;
	}

	/* The glass chip GlassButton fades in, stretched to the row. */
	.face {
		--panel-angle: 160deg;
		--panel-tint: color-mix(in srgb, var(--glassRim) 7%, transparent);
		--rim-hi: 0.45;
		--rim-mid: 0.12;
		--rim-lo: 0.05;
		--rim-end: 0.12;
		inset: 1px 0;
		border-radius: 8px;
		backdrop-filter: none;
		-webkit-backdrop-filter: none;
		box-shadow:
			1.5px 1.8px 5px rgba(0, 0, 0, 0.55),
			inset -4px -4px 6px -4px rgba(12, 26, 40, 0.25);
		opacity: 0;
		transition: opacity 0.15s ease;
		pointer-events: none;
	}

	.face::after {
		background: linear-gradient(
			var(--panel-angle),
			color-mix(in srgb, var(--glassRim) 22%, transparent) 0%,
			color-mix(in srgb, var(--glassRim) 6%, transparent) 33%,
			color-mix(in srgb, var(--glassRim) 0%, transparent) 55%
		);
	}

	.item:hover .face {
		opacity: 1;
	}

	.item:hover .apply {
		color: var(--color1);
	}

	/* Clear of the actions, which sit over the row's right end once shown. */
	.name {
		padding: 0 10px;
		transition: padding 0.15s ease;
	}

	.item:hover .name,
	.item:focus-within .name {
		padding-right: 84px;
	}

	.actions {
		right: 2px;
		opacity: 0;
		visibility: hidden;
		transition:
			opacity 0.15s ease,
			visibility 0s linear 0.15s;
	}

	.item:hover .actions,
	.item:focus-within .actions {
		opacity: 1;
		visibility: visible;
		transition:
			opacity 0.15s ease,
			visibility 0s linear 0s;
	}

	.actions :global(svg) {
		color: var(--color2);
		position: relative;
	}

	.delete.armed :global(svg) {
		color: #ff5a4f;
	}
</style>
