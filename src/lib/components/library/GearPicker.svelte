<!-- lib/components/library/GearPicker.svelte -->
<script lang="ts">
    // Picks the camera, scanner or chemistry a roll was shot, scanned or developed with.
    import { library } from "$lib/library/library.svelte";
    import type { RollRecord } from "$lib/library/catalogApi";
    import { type Gear, gearLabel, matchesGear, newGear } from "$lib/types/gear";
    import type { RollField } from "$lib/types/rollInfo";
    import { settings } from "$lib/settings/settings.svelte";
    import Combobox from "./Combobox.svelte";

    let { roll, slot, label }: { roll: RollRecord; slot: "camera" | "scanner" | "chemistry"; label: string } = $props();

    const NOUNS = { camera: "camera", scanner: "scanner", chemistry: "chemistry setup" };
    // What the roll keeps as text for gear the catalog does not know.
    let textKey = $derived<RollField>(slot === "chemistry" ? "developer" : slot);
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
        settings.show(slot === "chemistry" ? "home-lab" : "gear");
    }

    const meta = (g: Gear) => (slot === "chemistry" ? g.process : g.mount);
</script>

<Combobox
    items={library.gearOfKind(slot)}
    label={gearLabel}
    matches={matchesGear}
    {meta}
    {value}
    ariaLabel={label}
    placeholder={`Choose a ${NOUNS[slot]}`}
    noun={NOUNS[slot]}
    onpick={(g) => library.setRollGear(roll.id, slot, g)}
    onadd={add}
    onsetup={setUp}
    onclear={picked || value ? () => library.setRollGear(roll.id, slot, null) : undefined}
/>
