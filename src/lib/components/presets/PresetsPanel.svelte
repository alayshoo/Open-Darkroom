<!-- lib/components/presets/PresetsPanel.svelte -->
<script lang="ts">
	import GlassButton from "$lib/components/inputs/GlassButton.svelte";
	import PresetItem from "./PresetItem.svelte";
	import type { Preset } from "$lib/presets/presetDocument";

	let {
		list,
		canApply = true,
		onapply,
		onhover,
		onadd,
		onimport,
		onedit,
		onexport,
		ondelete,
	}: {
		list: Preset[];
		canApply?: boolean;
		onapply?: (preset: Preset) => void;
		onhover?: (preset: Preset | null) => void;
		onadd?: () => void;
		onimport?: () => void;
		onedit?: (preset: Preset) => void;
		onexport?: (preset: Preset) => void;
		ondelete?: (preset: Preset) => void;
	} = $props();
</script>

<div class="presets-panel glass flex flex-col h-full rounded-[12px]">
	<header class="flex items-center shrink-0 gap-1 ml-4 mr-2.5 mt-2.5 mb-1">
		<span class="title flex-1">Presets</span>
		<GlassButton label="Import presets" onclick={() => onimport?.()}>
			<svg width="28" height="28" viewBox="0 0 24 24">
				<path
					d="M12 5.5V13M9.2 10.2 12 13l2.8-2.8M7.5 12.5v4.5h9v-4.5"
					fill="none"
					stroke="currentColor"
					stroke-width="1.2"
					stroke-linecap="round"
					stroke-linejoin="round"
				/>
			</svg>
		</GlassButton>
		<GlassButton label="New preset from current settings" onclick={() => onadd?.()}>
			<svg width="28" height="28" viewBox="0 0 24 24">
				<path
					d="M12 7v10M7 12h10"
					fill="none"
					stroke="currentColor"
					stroke-width="1.3"
					stroke-linecap="round"
				/>
			</svg>
		</GlassButton>
	</header>
	<div class="list flex flex-col flex-1 min-h-0 px-2 pb-6" role="list">
		{#each list as preset (preset.id)}
			<PresetItem
				{preset}
				{canApply}
				onapply={() => onapply?.(preset)}
				onhover={(hovering) => onhover?.(hovering ? preset : null)}
				onedit={() => onedit?.(preset)}
				onexport={() => onexport?.(preset)}
				ondelete={() => ondelete?.(preset)}
			/>
		{:else}
			<p class="empty m-0 px-2 pt-2">
				No presets yet. Add one from the current settings, or import a
				.odpreset file.
			</p>
		{/each}
	</div>
</div>

<style>
	.presets-panel {
		--panel-angle: 146deg;
		text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
		box-shadow:
			10px 16px 38px -4px rgba(0, 0, 0, 0.51),
			3px 5px 12px -2px rgba(0, 0, 0, 0.36),
			inset 20px 30px 60px -42px rgba(255, 255, 255, 0.17);
	}

	.title {
		font-family: var(--font-ui);
		font-size: 18px;
		font-weight: 600;
		color: var(--color1);
	}

	header :global(svg) {
		color: var(--color2);
		position: relative;
	}

	.list {
		overflow-y: auto;
		scrollbar-width: none;
		mask-image: linear-gradient(
			to bottom,
			transparent 0,
			black 8px,
			black calc(100% - 24px),
			transparent 100%
		);
		-webkit-mask-image: linear-gradient(
			to bottom,
			transparent 0,
			black 8px,
			black calc(100% - 24px),
			transparent 100%
		);
	}

	.list::-webkit-scrollbar {
		display: none;
	}

	.empty {
		font-family: var(--font-ui);
		font-size: 13px;
		line-height: 1.45;
		color: var(--color3);
	}
</style>
