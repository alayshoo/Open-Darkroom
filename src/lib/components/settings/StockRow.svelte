<!-- lib/components/settings/StockRow.svelte -->
<script lang="ts">
    // One film stock in the settings list: a summary line that opens into its fields.
    import FilmCanister from "$lib/components/library/FilmCanister.svelte";
    import EditRow from "./EditRow.svelte";
    import { library } from "$lib/library/library.svelte";
    import {
        type FilmStock,
        type StockKind,
        STOCK_FORMATS,
        STOCK_KINDS,
        STOCK_PROCESSES,
        kindLabel,
        usualProcess,
    } from "$lib/types/filmStock";

    let { stock, open = false, ontoggle }: { stock: FilmStock; open?: boolean; ontoggle: () => void } = $props();

    let summary = $derived(
        [stock.boxIso ? `ISO ${stock.boxIso}` : null, kindLabel(stock.kind), stock.process, stock.formats.join(", ")]
            .filter(Boolean)
            .join(" · "),
    );

    const update = (patch: Partial<FilmStock>) => library.updateStock(stock.id, patch);

    function setKind(kind: StockKind) {
        // The process follows the kind while it is still the one the old kind implied.
        update(stock.process === usualProcess(stock.kind) ? { kind, process: usualProcess(kind) } : { kind });
    }

    function toggleFormat(format: string) {
        const formats = stock.formats.includes(format)
            ? stock.formats.filter((f) => f !== format)
            : STOCK_FORMATS.filter((f) => f === format || stock.formats.includes(f));
        update({ formats });
    }
</script>

{#snippet canister()}
    <FilmCanister colour={stock.colour} colour2={stock.colour2} colour3={stock.colour3} size={22} />
{/snippet}

<EditRow
    title={stock.name || "Untitled stock"}
    {summary}
    {open}
    {ontoggle}
    icon={canister}
    deleteLabel="Delete stock"
    ondelete={() => void library.deleteStock(stock.id)}
>
    <label class="field">
        <span>Maker</span>
        <input value={stock.maker} placeholder="Kodak" oninput={(e) => update({ maker: e.currentTarget.value })} />
    </label>
    <label class="field">
        <span>Name</span>
        <input value={stock.name} placeholder="Portra 400" oninput={(e) => update({ name: e.currentTarget.value })} />
    </label>
    <label class="field">
        <span>Box ISO</span>
        <input
            inputmode="numeric"
            value={stock.boxIso ?? ""}
            placeholder="400"
            oninput={(e) => {
                const n = parseInt(e.currentTarget.value, 10);
                update({ boxIso: Number.isFinite(n) && n > 0 ? n : null });
            }}
        />
    </label>
    <div class="field">
        <span>Colours</span>
        <div class="colours flex items-center">
            <input
                type="color"
                aria-label="First colour"
                value={stock.colour ?? "#808080"}
                oninput={(e) => update({ colour: e.currentTarget.value })}
            />
            {#if stock.colour2}
                <input
                    type="color"
                    aria-label="Second colour"
                    value={stock.colour2}
                    oninput={(e) => update({ colour2: e.currentTarget.value })}
                />
            {/if}
            {#if stock.colour3}
                <input
                    type="color"
                    aria-label="Third colour"
                    value={stock.colour3}
                    oninput={(e) => update({ colour3: e.currentTarget.value })}
                />
            {/if}
            {#if !stock.colour3}
                <button
                    class="text-button"
                    onclick={() =>
                        update(stock.colour2 ? { colour3: stock.colour2 } : { colour2: stock.colour ?? "#808080" })}
                >
                    Add a colour
                </button>
            {/if}
            {#if stock.colour2}
                <button
                    class="text-button"
                    onclick={() => update(stock.colour3 ? { colour3: null } : { colour2: null })}
                >
                    Remove the last
                </button>
            {/if}
        </div>
    </div>
    <div class="field wide">
        <span>Type</span>
        <div class="choice flex" role="radiogroup" aria-label="Type">
            {#each STOCK_KINDS as kind (kind.value)}
                <button role="radio" aria-checked={stock.kind === kind.value} class:on={stock.kind === kind.value} onclick={() => setKind(kind.value)}>
                    {kind.label}
                </button>
            {/each}
        </div>
    </div>
    <div class="field">
        <span>Process</span>
        <div class="choice flex" role="radiogroup" aria-label="Process">
            {#each STOCK_PROCESSES as process (process)}
                <button role="radio" aria-checked={stock.process === process} class:on={stock.process === process} onclick={() => update({ process })}>
                    {process}
                </button>
            {/each}
        </div>
    </div>
    <div class="field">
        <span>Formats</span>
        <div class="choice flex" role="group" aria-label="Formats">
            {#each STOCK_FORMATS as format (format)}
                <button aria-pressed={stock.formats.includes(format)} class:on={stock.formats.includes(format)} onclick={() => toggleFormat(format)}>
                    {format}
                </button>
            {/each}
        </div>
    </div>
</EditRow>

<style>
    .colours {
        gap: 8px;
        height: 30px;
    }

    input[type="color"] {
        width: 34px;
        height: 26px;
        padding: 0;
        border: 0;
        border-radius: 6px;
        background: transparent;
        cursor: default;
    }

    input[type="color"]::-webkit-color-swatch-wrapper {
        padding: 0;
    }

    input[type="color"]::-webkit-color-swatch {
        border: 0;
        border-radius: 6px;
    }
</style>
