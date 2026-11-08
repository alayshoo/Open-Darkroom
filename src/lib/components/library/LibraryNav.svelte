<!-- lib/components/library/LibraryNav.svelte -->
<script lang="ts">
    import { library } from "$lib/library/library.svelte";
    import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
    import FilmCanister from "./FilmCanister.svelte";

    let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

    function libraryMenu(e: MouseEvent, id: number) {
        e.preventDefault();
        menu = {
            x: e.clientX,
            y: e.clientY,
            items: [
                { label: "Rename", action: () => (library.renamingId = id) },
                { label: "New roll", action: () => void library.createRoll(id) },
                { separator: true },
                { label: "Delete library…", danger: true, action: () => library.deleteLibrary(id) },
            ],
        };
    }

    function rollMenu(e: MouseEvent, id: number) {
        e.preventDefault();
        menu = {
            x: e.clientX,
            y: e.clientY,
            items: [{ label: "Delete roll…", danger: true, action: () => library.deleteRoll(id) }],
        };
    }

    function focusAndSelect(node: HTMLInputElement) {
        node.focus();
        node.select();
    }

    function renameKey(e: KeyboardEvent, id: number) {
        if (e.key === "Enter") library.renameLibrary(id, (e.currentTarget as HTMLInputElement).value);
        else if (e.key === "Escape") library.renamingId = null;
        e.stopPropagation();
    }
</script>

<nav class="library-nav flex flex-col h-full">
    <header class="flex items-center shrink-0 ml-2.5 mr-1 mt-2.5 h-7">
        <span class="title flex-1">Catalog</span>
        <button class="add" title="New library" aria-label="New library" onclick={() => library.createLibrary()}>
            <svg width="18" height="18" viewBox="0 0 24 24">
                <path d="M12 5v14M5 12h14" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
            </svg>
        </button>
    </header>

    <div class="tree flex flex-col flex-1 min-h-0 pt-3 pr-1 pb-6">
        {#each library.libraries as lib (lib.id)}
            {@const current = library.view?.kind === "library" && library.view.id === lib.id}
            <div class="group flex flex-col">
                {#if library.renamingId === lib.id}
                    <input
                        class="rename"
                        value={lib.name}
                        aria-label="Library name"
                        use:focusAndSelect
                        onkeydown={(e) => renameKey(e, lib.id)}
                        onblur={(e) => library.renameLibrary(lib.id, e.currentTarget.value)}
                    />
                {:else}
                    <button
                        class="row library-row flex items-center"
                        class:current
                        aria-current={current ? "page" : undefined}
                        onclick={() => library.show({ kind: "library", id: lib.id })}
                        ondblclick={() => (library.renamingId = lib.id)}
                        oncontextmenu={(e) => libraryMenu(e, lib.id)}
                    >
                        <div class="face glass absolute"></div>
                        <svg class="glyph" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
                            <rect x="3.5" y="5.5" width="17" height="13" rx="2" fill="none" stroke="currentColor" stroke-width="1.4" />
                            <path d="M3.5 15l4.5-4 4 3.5 3-2.5 5 4" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" />
                        </svg>
                        <span class="name flex-1">{lib.name}</span>
                        <span class="count">{library.countIn(lib.id)}</span>
                    </button>
                {/if}
                {#each library.rollsOf(lib.id) as roll (roll.id)}
                    {@const currentRoll = library.view?.kind === "roll" && library.view.id === roll.id}
                    {@const stock = library.stockOf(roll)}
                    <button
                        class="row roll-row flex items-center"
                        class:current={currentRoll}
                        aria-current={currentRoll ? "page" : undefined}
                        onclick={() => library.show({ kind: "roll", id: roll.id })}
                        oncontextmenu={(e) => rollMenu(e, roll.id)}
                    >
                        <div class="face glass absolute"></div>
                        <span class="glyph roll-glyph flex"><FilmCanister colour={stock?.colour} colour2={stock?.colour2} colour3={stock?.colour3} /></span>
                        <span class="name flex-1">{roll.name}</span>
                        <span class="count">{library.framesOf(roll.id).length}</span>
                    </button>
                {/each}
            </div>
        {/each}

        {#if library.loaded && library.libraries.length === 0}
            <p class="note m-0 px-2.5">No libraries yet. Make one to start, or open an image and one is made for it.</p>
        {/if}
    </div>
</nav>

{#if menu}
    <ContextMenu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
    .library-nav {
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.65);
    }

    .title {
        font-family: var(--font-ui);
        font-size: 18px;
        font-weight: 600;
        color: var(--color1);
    }

    .add {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--color2);
    }

    .add:hover {
        background: var(--bg5);
        color: var(--color1);
    }

    .tree {
        overflow-y: auto;
        scrollbar-width: none;
        gap: 6px;
    }

    .tree::-webkit-scrollbar {
        display: none;
    }

    .row {
        position: relative;
        gap: 8px;
        height: 30px;
        padding: 0 10px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 14px;
        text-align: left;
        cursor: default;
        transition: background 0.12s ease, color 0.12s ease;
    }

    .row:hover,
    .row.current {
        color: var(--color1);
    }

    .row > :not(.face) {
        position: relative;
    }

    /* The presets' glass chip, held up under the selected row and half shown on hover. */
    .face {
        --panel-angle: 160deg;
        --panel-tint: color-mix(in srgb, var(--glassRim) 7%, transparent);
        --rim-hi: 0.45;
        --rim-mid: 0.12;
        --rim-lo: 0.05;
        --rim-end: 0.12;
        inset: 1px 0;
        border-radius: 8px;
        backdrop-filter: none;
        -webkit-backdrop-filter: none;
        box-shadow:
            1.5px 1.8px 5px rgba(0, 0, 0, 0.55),
            inset -4px -4px 6px -4px rgba(12, 26, 40, 0.25);
        opacity: 0;
        transition: opacity 0.15s ease;
        pointer-events: none;
    }

    .face::after {
        background: linear-gradient(
            var(--panel-angle),
            color-mix(in srgb, var(--glassRim) 22%, transparent) 0%,
            color-mix(in srgb, var(--glassRim) 6%, transparent) 33%,
            color-mix(in srgb, var(--glassRim) 0%, transparent) 55%
        );
    }

    .row:hover .face {
        opacity: 0.5;
    }

    .row.current .face {
        opacity: 1;
    }

    .library-row {
        font-weight: 600;
    }

    .roll-row {
        padding-left: 28px;
        font-size: 13px;
        height: 28px;
    }

    .glyph {
        flex-shrink: 0;
        opacity: 0.75;
    }

    /* At full strength, so a stock's colours read true. */
    .roll-glyph {
        opacity: 1;
    }

    .name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .count {
        font-size: 12px;
        color: var(--color3);
        font-variant-numeric: tabular-nums;
    }

    .rename {
        height: 30px;
        margin: 0;
        padding: 0 10px;
        border: 1px solid var(--color4);
        border-radius: 8px;
        background: var(--bg3);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 14px;
        font-weight: 600;
        outline: none;
    }

    .note {
        font-family: var(--font-ui);
        font-size: 13px;
        line-height: 1.45;
        color: var(--color3);
    }
</style>
