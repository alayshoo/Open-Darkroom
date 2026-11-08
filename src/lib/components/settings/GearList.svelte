<!-- lib/components/settings/GearList.svelte -->
<script lang="ts">
    // Every piece of gear of one kind, with a row to add another.
    import SettingsGroup from "./SettingsGroup.svelte";
    import GearRow from "./GearRow.svelte";
    import { library } from "$lib/library/library.svelte";
    import { tick } from "svelte";
    import { type GearKind, GEAR_KINDS, newGear } from "$lib/types/gear";

    let { kind, title }: { kind: GearKind; title?: string } = $props();

    let openId = $state<number | null>(null);
    let spec = $derived(GEAR_KINDS.find((k) => k.kind === kind)!);
    let list = $state<HTMLElement | null>(null);

    // Opens on gear just made from a roll, so it can be filled in.
    $effect(() => {
        const id = library.editingGearId;
        if (id === null || library.gearOf(id)?.kind !== kind) return;
        openId = id;
        library.editingGearId = null;
        tick().then(() => list?.querySelector(`[data-gear="${id}"]`)?.scrollIntoView({ block: "nearest" }));
    });

    async function add() {
        const gear = await library.createGear(newGear(kind));
        if (gear) openId = gear.id;
    }
</script>

<div class="contents" bind:this={list}>
    <SettingsGroup title={title ?? spec.title}>
        {#each library.gearOfKind(kind) as gear (gear.id)}
            <div data-gear={gear.id}>
                <GearRow {gear} open={openId === gear.id} ontoggle={() => (openId = openId === gear.id ? null : gear.id)} />
            </div>
        {/each}
        <button class="add flex items-center" onclick={add}>
            <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5v14M5 12h14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
            </svg>
            Add {kind === "chemistry" ? "a chemistry setup" : `a ${spec.one}`}
        </button>
    </SettingsGroup>
</div>

<style>
    .add {
        gap: 10px;
        width: 100%;
        height: 40px;
        padding: 0 12px;
        border: 0;
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 13px;
        text-align: left;
        cursor: default;
    }

    .add:hover {
        background: color-mix(in srgb, var(--glassRim) 4%, transparent);
        color: var(--color1);
    }
</style>
