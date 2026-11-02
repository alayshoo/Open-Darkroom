<!-- lib/components/presets/PresetModal.svelte -->
<script lang="ts">
	import GlassButton from "$lib/components/inputs/GlassButton.svelte";
	import SettingsToggle from "$lib/components/settings/SettingsToggle.svelte";
	import { PRESET_GROUPS, type PresetMode } from "$lib/presets/presetDocument";

	let {
		editing = false,
		initialName = "",
		initialGroups,
		onsave,
		oncancel,
	}: {
		// Modifying an existing preset rather than adding one.
		editing?: boolean;
		initialName?: string;
		initialGroups: Iterable<string>;
		onsave: (name: string, groups: Set<string>, overwrite: boolean) => void;
		oncancel: () => void;
	} = $props();

	// Seeded once from the props: the dialog is remounted for every preset.
	// svelte-ignore state_referenced_locally
	let name = $state(initialName);
	// svelte-ignore state_referenced_locally
	let selected = $state(new Set(initialGroups));
	let overwrite = $state(false);

	let canSave = $derived(name.trim().length > 0 && selected.size > 0);

	const COLUMNS: { mode: PresetMode; label: string }[] = [
		{ mode: "lighter-room", label: "Lighter Room" },
		{ mode: "darkroom", label: "Darkroom" },
		{ mode: "both", label: "Both Modes" },
	];

	function toggle(id: string) {
		const next = new Set(selected);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selected = next;
	}

	function setAll(on: boolean) {
		selected = on ? new Set(PRESET_GROUPS.map((g) => g.id)) : new Set();
	}

	function save() {
		if (canSave) onsave(name.trim(), selected, overwrite);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") oncancel();
		else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) save();
		else return;
		e.preventDefault();
		e.stopPropagation();
	}

	let dialogEl: HTMLElement | undefined = $state();
	let nameEl: HTMLInputElement | undefined = $state();

	$effect(() => {
		nameEl?.focus();
		nameEl?.select();
	});

	function handleOverlayClick(e: MouseEvent) {
		if (dialogEl && !dialogEl.contains(e.target as Node)) oncancel();
	}
</script>

<!-- Like the settings overlay, it paints and filters nothing itself, so the
     glass behind the scrim keeps sampling the app. -->
<div
	class="overlay fixed inset-0 z-1000 flex items-center justify-center"
	onclick={handleOverlayClick}
	onkeydown={handleKeydown}
	role="presentation"
>
	<div class="scrim absolute inset-0"></div>
	<div
		bind:this={dialogEl}
		class="dialog relative flex flex-col rounded-[14px]"
		role="dialog"
		aria-modal="true"
		aria-label={editing ? "Modify preset" : "New preset"}
	>
		<header class="flex items-center justify-between pl-5 pr-2.5 pt-2.5">
			<h2 class="title m-0">{editing ? "Modify Preset" : "New Preset"}</h2>
			<div class="close-slot">
				<GlassButton label="Cancel" onclick={oncancel}>
					<svg width="28" height="28" viewBox="0 0 24 24">
						<path
							d="M8.6 8.6l6.8 6.8M15.4 8.6l-6.8 6.8"
							fill="none"
							stroke="currentColor"
							stroke-width="1.4"
							stroke-linecap="round"
						/>
					</svg>
				</GlassButton>
			</div>
		</header>

		<div class="body flex flex-col gap-4 px-5 pt-3 pb-5">
			<label class="field flex flex-col gap-1.5">
				<span class="label">Name</span>
				<input
					bind:this={nameEl}
					bind:value={name}
					class="name-input h-8 px-2.5 rounded-[8px]"
					type="text"
					placeholder="My preset"
					spellcheck="false"
					maxlength="120"
				/>
			</label>

			<div class="flex flex-col gap-1.5">
				<div class="flex items-center justify-between">
					<span class="label">Include</span>
					<div class="flex gap-3">
						<button class="link border-0 p-0" onclick={() => setAll(true)}>All</button>
						<button class="link border-0 p-0" onclick={() => setAll(false)}>None</button>
					</div>
				</div>
				<div class="groups grid gap-x-6 gap-y-3">
					{#each COLUMNS as column (column.mode)}
						<div class="flex flex-col gap-1">
							<span class="column-title">{column.label}</span>
							{#each PRESET_GROUPS.filter((g) => g.mode === column.mode) as group (group.id)}
								<label class="group-row flex items-center gap-2 h-7 cursor-auto">
									<input
										type="checkbox"
										class="checkbox"
										checked={selected.has(group.id)}
										onchange={() => toggle(group.id)}
									/>
									<span>{group.label}</span>
								</label>
							{/each}
						</div>
					{/each}
				</div>
			</div>

			{#if editing}
				<div class="overwrite flex items-center justify-between gap-4">
					<div class="flex flex-col">
						<span class="option">Update values from the current image</span>
						<span class="hint">
							Off keeps the stored values. Newly included groups take
							the current image's values either way.
						</span>
					</div>
					<SettingsToggle
						checked={overwrite}
						label="Update values from the current image"
						onclick={() => (overwrite = !overwrite)}
					/>
				</div>
			{/if}

			<div class="footer flex justify-end gap-2 pt-1">
				<button class="action h-8 px-4 rounded-[8px] border-0" onclick={oncancel}>
					Cancel
				</button>
				<button
					class="action primary glass h-8 px-4 rounded-[8px] border-0"
					disabled={!canSave}
					onclick={save}
				>
					{editing ? "Save" : "Add Preset"}
				</button>
			</div>
		</div>
	</div>
</div>

<style>
	.overlay {
		padding: 44px 12px 12px;
	}

	.scrim {
		background: rgba(0, 0, 0, 0.6);
		backdrop-filter: blur(4px);
		animation: scrimIn 0.15s ease forwards;
	}

	.dialog {
		width: min(520px, 92vw);
		max-height: 90vh;
		overflow-y: auto;
		background: var(--bg3);
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.65);
		font-family: var(--font-ui);
		animation: dialogIn 0.18s cubic-bezier(0.2, 0, 0, 1) forwards;
	}

	@keyframes scrimIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	@keyframes dialogIn {
		from {
			transform: translateY(-6px) scale(0.985);
		}
		to {
			transform: translateY(0) scale(1);
		}
	}

	.title {
		font-family: "Outfit Variable", var(--font-ui);
		font-size: 25px;
		font-weight: 700;
		color: var(--color1);
	}

	.close-slot :global(svg) {
		color: var(--color2);
		position: relative;
	}

	.label,
	.column-title {
		font-size: 12px;
		font-weight: 600;
		letter-spacing: 0.03em;
		text-transform: uppercase;
		color: var(--color3);
	}

	.column-title {
		padding-bottom: 2px;
	}

	.name-input {
		background: var(--bg1);
		border: 1px solid var(--color5);
		color: var(--color1);
		font-family: inherit;
		font-size: 14px;
		outline: none;
		transition: border-color 0.15s ease;
	}

	.name-input:focus {
		border-color: var(--color3);
	}

	.groups {
		grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
	}

	.group-row {
		font-size: 13px;
		color: var(--color2);
	}

	.group-row:hover {
		color: var(--color1);
	}

	.checkbox {
		width: 14px;
		height: 14px;
		margin: 0;
		accent-color: var(--color1);
	}

	.link {
		background: transparent;
		color: var(--color3);
		font-family: inherit;
		font-size: 12px;
		cursor: auto;
	}

	.link:hover {
		color: var(--color1);
	}

	.option {
		font-size: 13px;
		color: var(--color1);
	}

	.hint {
		font-size: 12px;
		line-height: 1.4;
		color: var(--color3);
	}

	.action {
		background: transparent;
		color: var(--color2);
		font-family: inherit;
		font-size: 13px;
		cursor: auto;
		transition:
			transform 0.12s ease,
			color 0.15s ease,
			background 0.12s ease;
	}

	.action:not(.primary):hover {
		background: color-mix(in srgb, var(--glassRim) 6%, transparent);
		color: var(--color1);
	}

	.action.primary:hover {
		--panel-tint: color-mix(in srgb, var(--glassRim) 16%, transparent);
	}

	.action:active {
		transform: scale(0.95);
	}

	.action.primary {
		--panel-tint: color-mix(in srgb, var(--glassRim) 10%, transparent);
		background: var(--panel-noise), var(--panel-tint);
		backdrop-filter: none;
		-webkit-backdrop-filter: none;
		color: var(--color1);
	}

	.action:disabled {
		opacity: 0.35;
		pointer-events: none;
	}
</style>
