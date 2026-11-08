<!-- lib/components/library/LensPicker.svelte -->
<script lang="ts">
    // The lenses a roll was shot with: any number of them, those that fit its
    // camera offered first.
    import { library } from "$lib/library/library.svelte";
    import type { RollRecord } from "$lib/library/catalogApi";
    import { type Gear, gearLabel, matchesGear, newGear } from "$lib/types/gear";
    import Combobox from "./Combobox.svelte";

    let { roll }: { roll: RollRecord } = $props();

    let lenses = $derived(roll.gear.lenses.map((id) => library.gearOf(id)).filter((g): g is Gear => g !== null));
    // Lenses typed before they were in the catalog, or that were deleted from it.
    let unlisted = $derived(roll.info.lenses?.trim() ?? "");
    let mount = $derived(library.gearOf(roll.gear.camera)?.mount.trim().toLowerCase() ?? "");
    let offered = $derived.by(() => {
        const left = library.gearOfKind("lens").filter((l) => !roll.gear.lenses.includes(l.id));
        const fits = (l: Gear) => mount !== "" && l.mount.trim().toLowerCase() === mount;
        return [...left.filter(fits), ...left.filter((l) => !fits(l))];
    });

    async function add(text: string) {
        const created = await library.createGear(newGear("lens", text));
        if (created) library.addRollLens(roll.id, created);
    }
</script>

<div class="lenses flex flex-col">
    {#if lenses.length > 0 || unlisted}
        <ul class="chips m-0 p-0 flex flex-wrap">
            {#each lenses as lens (lens.id)}
                <li class="chip flex items-center">
                    <span class="name">{gearLabel(lens)}</span>
                    <button aria-label={`Remove ${gearLabel(lens)}`} onclick={() => library.removeRollLens(roll.id, lens.id)}>
                        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                        </svg>
                    </button>
                </li>
            {/each}
            {#if unlisted}
                <li class="chip unlisted flex items-center">
                    <span class="name">{unlisted}</span>
                    <button aria-label={`Remove ${unlisted}`} onclick={() => library.updateRoll(roll.id, { info: { lenses: "" } })}>
                        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
                            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
                        </svg>
                    </button>
                </li>
            {/if}
        </ul>
    {/if}
    <Combobox
        items={offered}
        label={gearLabel}
        matches={matchesGear}
        meta={(l) => l.mount}
        value=""
        ariaLabel="Add a lens"
        placeholder="Add a lens"
        noun="lens"
        onpick={(l) => library.addRollLens(roll.id, l)}
        onadd={add}
    />
</div>

<style>
    .lenses {
        gap: 6px;
    }

    .chips {
        gap: 5px;
        list-style: none;
    }

    .chip {
        gap: 4px;
        max-width: 100%;
        height: 26px;
        padding: 0 4px 0 9px;
        border-radius: 7px;
        background: var(--bg5);
        color: var(--color1);
        font-size: 12.5px;
    }

    .unlisted {
        background: transparent;
        box-shadow: inset 0 0 0 1px var(--bg5);
        color: var(--color2);
    }

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .chip button {
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 18px;
        height: 18px;
        padding: 0;
        border: 0;
        border-radius: 5px;
        background: transparent;
        color: var(--color3);
        cursor: default;
    }

    .chip button:hover {
        background: rgba(255, 255, 255, 0.08);
        color: var(--color1);
    }
</style>
