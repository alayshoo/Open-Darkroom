<!-- lib/components/settings/sections/FilmStocksSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import StockRow from "../StockRow.svelte";
    import AddRow from "../AddRow.svelte";
    import EditableName from "$lib/components/library/EditableName.svelte";

    import { tick } from "svelte";
    import { library } from "$lib/library/library.svelte";
    import type { FilmStock } from "$lib/types/filmStock";

    let openId = $state<number | null>(null);
    // Makers added here that have no stocks yet; they last until one is added or Settings closes.
    let newMakers = $state<string[]>([]);
    let naming = $state(false);
    let section = $state<HTMLElement | null>(null);

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
        if (!maker) return;
        if (!groups.some((g) => g.maker.toLowerCase() === maker.toLowerCase())) newMakers = [...newMakers, maker];
        // Its group sits in alphabetical order, usually far from this row, so bring it into view.
        tick().then(() =>
            section
                ?.querySelector(`[data-maker="${CSS.escape(maker.toLowerCase())}"]`)
                ?.scrollIntoView({ block: "center", behavior: "smooth" }),
        );
    }

    /** Renames a maker on every stock it has; a name already in the list merges the two. */
    function renameMaker(from: string, to: string) {
        const key = from.toLowerCase();
        for (const stock of library.filmStocks) {
            if (stock.maker.trim().toLowerCase() === key) library.updateStock(stock.id, { maker: to });
        }
        newMakers = newMakers.map((m) => (m.toLowerCase() === key ? to : m));
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

<div class="section flex flex-col gap-4" bind:this={section}>
    {#each groups as { maker, stocks } (maker.toLowerCase())}
        <div data-maker={maker.toLowerCase()}>
            {#snippet heading()}
                <EditableName value={maker || "Other"} label="Film maker" onrename={(name) => renameMaker(maker, name)} />
            {/snippet}
            <SettingsGroup {heading}>
                {#each stocks as stock (stock.id)}
                    <StockRow {stock} open={openId === stock.id} ontoggle={() => (openId = openId === stock.id ? null : stock.id)} />
                {/each}
                <AddRow label="Add a stock" onclick={() => add(maker)} />
            </SettingsGroup>
        </div>
    {/each}
    <SettingsGroup>
        {#if naming}
            <div class="naming flex items-center">
                <input
                    aria-label="Film maker"
                    placeholder="Maker name"
                    use:focus
                    onkeydown={nameMaker}
                    onblur={() => (naming = false)}
                />
                <span class="hint">Enter to add</span>
            </div>
        {:else}
            <AddRow label="Add a film maker" onclick={() => (naming = true)} />
        {/if}
    </SettingsGroup>
</div>

<style>
    .naming {
        gap: 12px;
        height: 40px;
        padding: 0 12px 0 30px;
    }

    input {
        flex: 1;
        min-width: 0;
        height: 28px;
        padding: 0 8px;
        border: 1px solid var(--color4);
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        outline: none;
    }

    input::placeholder {
        color: var(--color4);
    }

    .hint {
        flex-shrink: 0;
        font-size: 12px;
        color: var(--color3);
    }
</style>
