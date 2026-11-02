<!-- lib/components/presets/PresetModal.svelte -->
<script lang="ts">
	import GlassButton from "$lib/components/inputs/GlassButton.svelte";
	import SettingsToggle from "$lib/components/settings/SettingsToggle.svelte";
	import {
		PRESET_FIELDS,
		PRESET_GROUPS,
		type PresetField,
		type PresetMode,
	} from "$lib/presets/presetDocument";

	let {
		editing = false,
		initialName = "",
		initialFields,
		onsave,
		oncancel,
	}: {
		// Modifying an existing preset rather than adding one.
		editing?: boolean;
		initialName?: string;
		initialFields: Iterable<string>;
		onsave: (name: string, fields: Set<string>, overwrite: boolean) => void;
		oncancel: () => void;
	} = $props();

	// Seeded once from the props: the dialog is remounted for every preset.
	// svelte-ignore state_referenced_locally
	let name = $state(initialName);
	// svelte-ignore state_referenced_locally
	let selected = $state(new Set(initialFields));
	let overwrite = $state(false);

	let canSave = $derived(name.trim().length > 0 && selected.size > 0);

	const SECTIONS: { mode: PresetMode; label: string }[] = [
		{ mode: "lighter-room", label: "Lighter Room" },
		{ mode: "darkroom", label: "Darkroom" },
		{ mode: "both", label: "Both Modes" },
	];

	function countSelected(fields: PresetField[]) {
		return fields.filter((f) => selected.has(f.id)).length;
	}

	function setFields(fields: PresetField[], on: boolean) {
		const next = new Set(selected);
		for (const f of fields) {
			if (on) next.add(f.id);
			else next.delete(f.id);
		}
		selected = next;
	}

	// A partly ticked set fills up; a full one empties.
	function toggleFields(fields: PresetField[]) {
		setFields(fields, countSelected(fields) < fields.length);
	}

	function indeterminate(node: HTMLInputElement, value: boolean) {
		node.indeterminate = value;
		return {
			update(next: boolean) {
				node.indeterminate = next;
			},
		};
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

{#snippet setBox(fields: PresetField[], label: string, strong: boolean)}
	{@const count = countSelected(fields)}
	<label class="check-row flex items-center gap-2 cursor-auto" class:strong>
		<input
			type="checkbox"
			class="checkbox"
			checked={count === fields.length}
			use:indeterminate={count > 0 && count < fields.length}
			onchange={() => toggleFields(fields)}
		/>
		<span>{label}</span>
	</label>
{/snippet}

{#snippet fieldBox(field: PresetField)}
	<label class="check-row flex items-center gap-2 cursor-auto">
		<input
			type="checkbox"
			class="checkbox"
			checked={selected.has(field.id)}
			onchange={() => setFields([field], !selected.has(field.id))}
		/>
		<span>{field.label}</span>
	</label>
{/snippet}

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
		<header class="flex items-center justify-between shrink-0 pl-5 pr-2.5 pt-2.5 pb-1">
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

		<div class="body flex flex-col flex-1 min-h-0 gap-4 px-5 pt-3 pb-4">
			<label class="flex flex-col gap-1.5">
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

			<div class="flex items-center justify-between">
				<span class="label">Include</span>
				<div class="flex gap-3">
					<button class="link border-0 p-0" onclick={() => setFields(PRESET_FIELDS, true)}>All</button>
					<button class="link border-0 p-0" onclick={() => setFields(PRESET_FIELDS, false)}>None</button>
				</div>
			</div>

			{#each SECTIONS as section (section.mode)}
				{@const groups = PRESET_GROUPS.filter((g) => g.mode === section.mode)}
				<section class="flex flex-col gap-2">
					{@render setBox(groups.flatMap((g) => g.fields), section.label, true)}
					<div class="groups grid gap-2">
						{#each groups as group (group.id)}
							<div
								class="group-card flex flex-col gap-0.5 rounded-[10px] px-3 py-2"
								class:wide={group.rows}
							>
								{@render setBox(group.fields, group.label, true)}
								<!-- A lone slider is its group's own checkbox. -->
								{#if group.fields.length > 1}
								<div class="fields flex flex-col gap-0.5">
									{#if group.rows}
										{#each group.rows as row (row.label)}
											<div class="band flex items-center gap-3">
												<div class="band-label">
													{@render setBox(row.fields, row.label, false)}
												</div>
												{#each row.fields as field (field.id)}
													{@render fieldBox(field)}
												{/each}
											</div>
										{/each}
									{:else}
										{#each group.fields as field (field.id)}
											{@render fieldBox(field)}
										{/each}
									{/if}
								</div>
								{/if}
							</div>
						{/each}
					</div>
				</section>
			{/each}
		</div>

		<div class="footer flex flex-col shrink-0 gap-3 px-5 pt-3 pb-4">
			{#if editing}
				<div class="flex items-center justify-between gap-4">
					<div class="flex flex-col">
						<span class="option">Update values from the current image</span>
						<span class="hint">
							Off keeps the stored values. Newly ticked sliders take the
							current image's values either way.
						</span>
					</div>
					<SettingsToggle
						checked={overwrite}
						label="Update values from the current image"
						onclick={() => (overwrite = !overwrite)}
					/>
				</div>
			{/if}
			<div class="flex justify-end gap-2">
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
		width: min(860px, 94vw);
		max-height: 90vh;
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

	.label {
		font-size: 13px;
		font-weight: 600;
		color: var(--color3);
	}

	.body {
		overflow-y: auto;
		scrollbar-width: none;
		mask-image: linear-gradient(
			to bottom,
			transparent 0,
			black 12px,
			black calc(100% - 16px),
			transparent 100%
		);
		-webkit-mask-image: linear-gradient(
			to bottom,
			transparent 0,
			black 12px,
			black calc(100% - 16px),
			transparent 100%
		);
	}

	.body::-webkit-scrollbar {
		display: none;
	}

	.footer {
		border-top: 1px solid var(--color5);
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
		grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
		align-items: start;
	}

	.group-card {
		background: color-mix(in srgb, var(--glassRim) 3%, transparent);
	}

	.group-card.wide {
		grid-column: span 2;
	}

	.fields {
		padding-left: 22px;
	}

	.band-label {
		width: 76px;
	}

	.check-row {
		height: 24px;
		font-size: 13px;
		color: var(--color2);
		white-space: nowrap;
	}

	.check-row:hover {
		color: var(--color1);
	}

	.check-row.strong {
		font-weight: 600;
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
