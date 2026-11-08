<!-- lib/components/library/StockPicker.svelte -->
<script lang="ts">
    // Picks the stock a roll was shot on.
    import { library } from "$lib/library/library.svelte";
    import type { RollRecord } from "$lib/library/catalogApi";
    import { type FilmStock, matchesStock, stockFromText, stockLabel } from "$lib/types/filmStock";
    import Combobox from "./Combobox.svelte";
    import FilmCanister from "./FilmCanister.svelte";

    let { roll }: { roll: RollRecord } = $props();

    let stock = $derived(library.stockOf(roll));

    async function add(text: string) {
        const created = await library.createStock(stockFromText(text));
        if (created) library.setRollStock(roll.id, created);
    }
</script>

{#snippet canister(option: FilmStock | null)}
    {@const shown = option ?? stock}
    <FilmCanister colour={shown?.colour} colour2={shown?.colour2} colour3={shown?.colour3} size={18} />
{/snippet}

<Combobox
    items={library.filmStocks}
    label={stockLabel}
    matches={matchesStock}
    meta={(s) => s.process}
    icon={canister}
    value={library.stockNameOf(roll)}
    ariaLabel="Film stock"
    placeholder="Choose a stock"
    noun="stock"
    onpick={(s) => library.setRollStock(roll.id, s)}
    onadd={add}
    onclear={roll.stockId !== null || roll.info.filmStock ? () => library.setRollStock(roll.id, null) : undefined}
/>
