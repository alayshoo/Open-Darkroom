<!-- lib/components/settings/StockRow.svelte -->
<script lang="ts">
    // One film stock in the settings list: a summary line that opens into its fields.
    import FilmCanister from "$lib/components/library/FilmCanister.svelte";
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

    let armed = $state(false);
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

    function remove() {
        if (!armed) {
            armed = true;
            return;
        }
        void library.deleteStock(stock.id);
    }
</script>

<div class="stock flex flex-col" class:open>
    <button class="line flex items-center" aria-expanded={open} onclick={ontoggle}>
        <FilmCanister colour={stock.colour} colour2={stock.colour2} colour3={stock.colour3} size={22} />
        <span class="name">{stock.name || "Untitled stock"}</span>
        <span class="summary flex-1">{summary}</span>
        <svg class="chevron" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M9 6l6 6-6 6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
    </button>

    {#if open}
        <div class="fields grid">
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
            <div class="footer wide flex items-center justify-end">
                <button class="text-button danger" class:armed onclick={remove} onblur={() => (armed = false)}>
                    {armed ? "Click again to delete" : "Delete stock"}
                </button>
            </div>
        </div>
    {/if}
</div>

<style>
    .line {
        gap: 10px;
        width: 100%;
        min-height: 44px;
        padding: 0 12px;
        border: 0;
        background: transparent;
        color: var(--color1);
        font-family: var(--font-ui);
        text-align: left;
        cursor: default;
    }

    .line:hover {
        background: color-mix(in srgb, var(--glassRim) 4%, transparent);
    }

    .name {
        font-size: 13.5px;
        white-space: nowrap;
    }

    .summary {
        font-size: 12px;
        color: var(--color3);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .chevron {
        color: var(--color3);
        flex-shrink: 0;
        transition: transform 0.15s ease;
    }

    .open .chevron {
        transform: rotate(90deg);
    }

    .fields {
        grid-template-columns: 1fr 1fr;
        gap: 12px 16px;
        padding: 4px 12px 14px 44px;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: 4px;
        min-width: 0;
        font-size: 12px;
        color: var(--color2);
    }

    .wide {
        grid-column: 1 / -1;
    }

    input:not([type="color"]) {
        height: 30px;
        padding: 0 9px;
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        outline: none;
    }

    input:not([type="color"]):focus {
        border-color: rgba(255, 255, 255, 0.28);
    }

    input::placeholder {
        color: var(--color4);
    }

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

    .choice {
        gap: 4px;
        padding: 3px;
        border-radius: 8px;
        background: rgba(0, 0, 0, 0.3);
    }

    .choice button {
        flex: 1;
        height: 24px;
        padding: 0 8px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--color2);
        font-size: 12px;
        white-space: nowrap;
        cursor: default;
    }

    .choice button.on {
        background: var(--bg5);
        color: var(--color1);
    }

    .text-button {
        height: 26px;
        padding: 0 8px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 12px;
        cursor: default;
    }

    .text-button:hover {
        background: var(--bg5);
        color: var(--color1);
    }

    .danger.armed,
    .danger:hover {
        color: #ff8a80;
    }
</style>
