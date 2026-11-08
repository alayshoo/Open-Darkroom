<!-- lib/components/settings/sections/FilmStocksSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import StockRow from "../StockRow.svelte";
    import AddRow from "../AddRow.svelte";

    import { library } from "$lib/library/library.svelte";
    import type { FilmStock } from "$lib/types/filmStock";

    let openId = $state<number | null>(null);
    // Makers added here that have no stocks yet; they last until one is added or Settings closes.
    let newMakers = $state<string[]>([]);
    let naming = $state(false);

    // One group per maker, matched regardless of case and spelled as its first stock spells it,
    // in alphabetical order; stocks with no maker last.
    let groups = $derived.by(() => {
        const byMaker = new Map<string, { maker: string; stocks: FilmStock[] }>();
        for (const stock of library.filmStocks) {
            const maker = stock.maker.trim();
            const group = byMaker.get(maker.toLowerCase()) ?? { maker, stocks: [] };
            group.stocks.push(stock);
            byMaker.set(maker.toLowerCase(), group);
        }
        for (const maker of newMakers) {
            if (!byMaker.has(maker.toLowerCase())) byMaker.set(maker.toLowerCase(), { maker, stocks: [] });
        }
        return [...byMaker.values()].sort((a, b) =>
            a.maker === "" ? 1 : b.maker === "" ? -1 : a.maker.localeCompare(b.maker),
        );
    });

    function nameMaker(e: KeyboardEvent) {
        const input = e.currentTarget as HTMLInputElement;
        e.stopPropagation();
        if (e.key === "Escape") naming = false;
        if (e.key !== "Enter") return;
        const maker = input.value.trim();
        naming = false;
        if (maker && !groups.some((g) => g.maker.toLowerCase() === maker.toLowerCase())) newMakers = [...newMakers, maker];
    }

    function focus(node: HTMLInputElement) {
        node.focus();
    }

    async function add(maker = "") {
        const stock = await library.createStock({
            maker,
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
    {#each groups as { maker, stocks } (maker.toLowerCase())}
        <SettingsGroup title={maker || "Other"}>
            {#each stocks as stock (stock.id)}
                <StockRow {stock} open={openId === stock.id} ontoggle={() => (openId = openId === stock.id ? null : stock.id)} />
            {/each}
            <AddRow label="Add a stock" onclick={() => add(maker)} />
        </SettingsGroup>
    {/each}
    <SettingsGroup>
        {#if naming}
            <input class="maker-name" aria-label="Film maker" placeholder="Film maker" use:focus onkeydown={nameMaker} onblur={() => (naming = false)} />
        {:else}
            <AddRow label="Add a film maker" onclick={() => (naming = true)} />
        {/if}
    </SettingsGroup>
</div>

<style>
    .maker-name {
        height: 40px;
        padding: 0 12px 0 36px;
        border: 0;
        background: transparent;
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        outline: none;
    }

    .maker-name::placeholder {
        color: var(--color4);
    }
</style>
