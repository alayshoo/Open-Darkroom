<script lang="ts">
	import GlassButton from "./GlassButton.svelte";
	import { retouchTool } from "$lib/retouch/retouchTool.svelte";
	import type { RetouchMode } from "$lib/types/retouch";

	let {
		disabled = false,
		ontoggle,
	}: {
		disabled?: boolean;
		ontoggle?: (mode: RetouchMode) => void;
	} = $props();

	let mode = $derived(retouchTool.mode);
</script>

<div class="retouch-tools flex flex-col items-center gap-1">
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
</div>

<style>
	.retouch-tools :global(svg) {
		color: var(--chromeInk);
		position: relative;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.65));
	}
</style>
