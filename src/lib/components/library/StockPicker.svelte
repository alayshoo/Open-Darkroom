<!-- lib/components/library/StockPicker.svelte -->
<script lang="ts">
    // Picks the stock a roll was shot on: type to narrow the list, or add what
    // was typed as a new stock.
    import { library } from "$lib/library/library.svelte";
    import type { RollRecord } from "$lib/library/catalogApi";
    import { type FilmStock, matchesStock, stockFromText, stockLabel } from "$lib/types/filmStock";
    import FilmCanister from "./FilmCanister.svelte";

    let { roll }: { roll: RollRecord } = $props();

    let open = $state(false);
    let query = $state("");
    let active = $state(0);
    let input = $state<HTMLInputElement | null>(null);

    let stock = $derived(library.stockOf(roll));
    let shown = $derived(library.stockNameOf(roll));
    let matches = $derived(library.filmStocks.filter((s) => matchesStock(s, query)));
    let exact = $derived(matches.some((s) => stockLabel(s).toLowerCase() === query.trim().toLowerCase()));
    let canAdd = $derived(query.trim() !== "" && !exact);
    // The rows the arrow keys walk: each match, then adding, then clearing.
    let options = $derived<("add" | "clear" | FilmStock)[]>([
        ...matches,
        ...(canAdd ? (["add"] as const) : []),
        ...(roll.stockId !== null || roll.info.filmStock ? (["clear"] as const) : []),
    ]);

    function start() {
        query = "";
        active = 0;
        open = true;
    }

    function close() {
        open = false;
        query = "";
    }

    async function choose(option: "add" | "clear" | FilmStock) {
        close();
        input?.blur();
        if (option === "clear") return library.setRollStock(roll.id, null);
        if (option === "add") {
            const created = await library.createStock(stockFromText(query || ""));
            if (created) library.setRollStock(roll.id, created);
            return;
        }
        library.setRollStock(roll.id, option);
    }

    function onKey(e: KeyboardEvent) {
        if (!open) return;
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            const step = e.key === "ArrowDown" ? 1 : -1;
            active = (active + step + options.length) % Math.max(options.length, 1);
        } else if (e.key === "Enter" && options[active]) {
            void choose(options[active]);
        } else if (e.key === "Escape") {
            close();
            input?.blur();
        } else {
            return;
        }
        e.preventDefault();
        e.stopPropagation();
    }
</script>

<div class="picker relative">
    <div class="field-box flex items-center">
        <FilmCanister colour={stock?.colour} colour2={stock?.colour2} colour3={stock?.colour3} size={18} />
        <input
            bind:this={input}
            role="combobox"
            aria-expanded={open}
            aria-controls="stock-options"
            aria-label="Film stock"
            placeholder={open ? "Search or add a stock" : "Choose a stock"}
            value={open ? query : shown}
            onfocus={start}
            onblur={() => setTimeout(close, 120)}
            oninput={(e) => {
                query = e.currentTarget.value;
                active = 0;
            }}
            onkeydown={onKey}
        />
    </div>

    {#if open}
        <div class="options" id="stock-options" role="listbox">
            {#each options as option, i (typeof option === "string" ? option : option.id)}
                <button
                    class="option flex items-center"
                    class:active={i === active}
                    role="option"
                    aria-selected={i === active}
                    onpointerenter={() => (active = i)}
                    onmousedown={(e) => e.preventDefault()}
                    onclick={() => choose(option)}
                >
                    {#if option === "add"}
                        <span class="plus">+</span>
                        <span class="label">Add “{query.trim()}” as a new stock</span>
                    {:else if option === "clear"}
                        <span class="label muted">No stock</span>
                    {:else}
                        <FilmCanister colour={option.colour} colour2={option.colour2} colour3={option.colour3} size={18} />
                        <span class="label flex-1">{stockLabel(option)}</span>
                        <span class="meta">{option.process}</span>
                    {/if}
                </button>
            {/each}
            {#if options.length === 0}
                <p class="empty">Type a name to add a stock.</p>
            {/if}
        </div>
    {/if}
</div>

<style>
    .field-box {
        gap: 7px;
        height: 30px;
        padding: 0 8px;
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
        color: var(--color2);
        transition: border-color 0.12s ease;
    }

    .field-box:focus-within {
        border-color: rgba(255, 255, 255, 0.28);
    }

    input {
        flex: 1;
        min-width: 0;
        height: 100%;
        padding: 0;
        border: 0;
        background: transparent;
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        outline: none;
    }

    input::placeholder {
        color: var(--color4);
    }

    .options {
        position: absolute;
        z-index: 20;
        top: calc(100% + 4px);
        left: 0;
        right: 0;
        max-height: 260px;
        overflow-y: auto;
        padding: 4px;
        border: 1px solid var(--bg5);
        border-radius: 10px;
        background: var(--bg3);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
        scrollbar-width: thin;
    }

    .option {
        gap: 8px;
        width: 100%;
        height: 30px;
        padding: 0 8px;
        border: 0;
        border-radius: 7px;
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 13px;
        text-align: left;
        cursor: default;
    }

    .option.active {
        background: var(--bg5);
        color: var(--color1);
    }

    .label {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .muted {
        color: var(--color3);
    }

    .meta {
        font-size: 11.5px;
        color: var(--color3);
    }

    .plus {
        width: 18px;
        text-align: center;
        font-size: 16px;
        color: var(--color2);
    }

    .empty {
        margin: 0;
        padding: 6px 8px;
        font-size: 12px;
        color: var(--color3);
    }
</style>
