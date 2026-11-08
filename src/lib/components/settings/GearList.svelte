<!-- lib/components/settings/GearList.svelte -->
<script lang="ts">
    // Every piece of gear of one kind, with a row to add another.
    import SettingsGroup from "./SettingsGroup.svelte";
    import GearRow from "./GearRow.svelte";
    import AddRow from "./AddRow.svelte";
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
        <AddRow label={kind === "chemistry" ? "Add a chemistry setup" : `Add a ${spec.one}`} onclick={add} />
    </SettingsGroup>
</div>

