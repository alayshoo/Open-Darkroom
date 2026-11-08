<!-- lib/components/settings/sections/FilmStocksSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import StockRow from "../StockRow.svelte";

    import { library } from "$lib/library/library.svelte";
    import type { FilmStock } from "$lib/types/filmStock";

    let openId = $state<number | null>(null);

    // One group per maker, in the order the catalog lists them; stocks with no maker last.
    let groups = $derived.by(() => {
        const byMaker = new Map<string, FilmStock[]>();
        for (const stock of library.filmStocks) {
            const maker = stock.maker.trim();
            byMaker.set(maker, [...(byMaker.get(maker) ?? []), stock]);
        }
        return [...byMaker].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
    });

    async function add() {
        const stock = await library.createStock({
            maker: "",
            name: "",
            boxIso: null,
            kind: "colourNegative",
            process: "C-41",
            formats: ["35mm"],
            colour: "#808080",
            colour2: null,
            colour3: null,
        });
        if (stock) openId = stock.id;
    }
</script>

<div class="section flex flex-col gap-4">
    <div class="intro flex items-center">
        <p class="m-0 flex-1">The stocks a roll can be shot on. They are kept in the catalog and go with it.</p>
        <button class="add" onclick={add}>Add film stock</button>
    </div>

    {#each groups as [maker, stocks] (maker)}
        <SettingsGroup title={maker || "Other"}>
            {#each stocks as stock (stock.id)}
                <StockRow {stock} open={openId === stock.id} ontoggle={() => (openId = openId === stock.id ? null : stock.id)} />
            {/each}
        </SettingsGroup>
    {/each}
</div>

<style>
    .intro {
        gap: 16px;
        padding: 0 12px;
        font-size: 12.5px;
        color: var(--color3);
    }

    .add {
        height: 28px;
        padding: 0 12px;
        border: 0;
        border-radius: 8px;
        background: var(--bg5);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        white-space: nowrap;
        cursor: default;
    }

    .add:hover {
        background: var(--color5);
    }
</style>
