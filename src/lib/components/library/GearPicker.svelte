<!-- lib/components/library/GearPicker.svelte -->
<script lang="ts">
    // Picks the camera, scanner, chemistry or lab a roll was shot, scanned or developed with.
    // A scanner is only a name, so it is added from what is typed; the others are set up in Settings.
    import { library } from "$lib/library/library.svelte";
    import type { RollRecord } from "$lib/library/catalogApi";
    import { type Gear, type GearSlot, GEAR_KINDS, GEAR_SLOTS, gearLabel, matchesGear, newGear } from "$lib/types/gear";
    import { settings } from "$lib/settings/settings.svelte";
    import Combobox from "./Combobox.svelte";

    let { roll, slot, label }: { roll: RollRecord; slot: GearSlot; label: string } = $props();

    let noun = $derived(GEAR_KINDS.find((k) => k.kind === slot)!.one);
    // What the roll keeps as text for gear the catalog does not know.
    let textKey = $derived(GEAR_SLOTS[slot]);
    let picked = $derived(library.gearOf(roll.gear[slot]));
    let value = $derived(picked ? gearLabel(picked) : (roll.info[textKey] ?? ""));

    async function add(text: string) {
        const created = await library.createGear(newGear(slot, text));
        if (created) library.setRollGear(roll.id, slot, created);
    }

    async function setUp(text: string) {
        const created = await library.createGear(newGear(slot, text));
        if (!created) return;
        library.setRollGear(roll.id, slot, created);
        library.editingGearId = created.id;
        settings.show(slot === "chemistry" || slot === "lab" ? "development" : "gear");
    }

    const meta = (g: Gear) => (slot === "chemistry" ? g.process : slot === "lab" ? g.location : g.mount);
</script>

<Combobox
    items={library.gearOfKind(slot)}
    label={gearLabel}
    matches={matchesGear}
    {meta}
    {value}
    ariaLabel={label}
    placeholder={`Choose a ${noun}`}
    {noun}
    onpick={(g) => library.setRollGear(roll.id, slot, g)}
    onadd={slot === "scanner" ? add : undefined}
    onsetup={slot === "scanner" ? undefined : setUp}
    onclear={picked || value ? () => library.setRollGear(roll.id, slot, null) : undefined}
/>
