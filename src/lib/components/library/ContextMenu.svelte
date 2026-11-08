<!-- lib/components/library/ContextMenu.svelte -->
<script lang="ts" module>
    export type MenuItem =
        | { separator: true }
        | { label: string; action: () => void; disabled?: boolean; indent?: number; danger?: boolean }
        | { heading: string };
</script>

<script lang="ts">
    import { onMount } from "svelte";

    let {
        x,
        y,
        items,
        onclose,
    }: {
        x: number;
        y: number;
        items: MenuItem[];
        onclose: () => void;
    } = $props();

    let menu = $state<HTMLDivElement | null>(null);
    // Where it opens, until it has been measured against the window.
    let pos = $state<{ left: number; top: number } | null>(null);

    // Kept inside the window, flipping up or left when it would run off it.
    onMount(() => {
        if (!menu) return;
        const r = menu.getBoundingClientRect();
        pos = {
            left: x + r.width > window.innerWidth - 8 ? Math.max(8, x - r.width) : x,
            top: y + r.height > window.innerHeight - 8 ? Math.max(8, y - r.height) : y,
        };
        menu.querySelector<HTMLButtonElement>("button")?.focus();
    });

    function onWindowPointer(e: PointerEvent) {
        if (menu && !menu.contains(e.target as Node)) onclose();
    }

    function onKey(e: KeyboardEvent) {
        if (e.key === "Escape") {
            e.stopPropagation();
            onclose();
        }
    }
</script>

<svelte:window onpointerdowncapture={onWindowPointer} onkeydowncapture={onKey} onblur={onclose} />

<div
    bind:this={menu}
    class="context-menu fixed p-1 rounded-[10px] border border-bg5 flex flex-col"
    style="left: {pos?.left ?? x}px; top: {pos?.top ?? y}px;"
    role="menu"
>
    {#each items as item, i (i)}
        {#if "separator" in item}
            <div class="separator h-0.25 mx-2 my-1"></div>
        {:else if "heading" in item}
            <div class="heading px-2.5 pt-1.5 pb-1">{item.heading}</div>
        {:else}
            <button
                class="item flex items-center w-full h-7.5 px-2.5 border-0 rounded-[8px]"
                class:danger={item.danger}
                style="padding-left: {10 + (item.indent ?? 0) * 14}px"
                role="menuitem"
                disabled={item.disabled}
                onclick={() => {
                    onclose();
                    item.action();
                }}
            >
                {item.label}
            </button>
        {/if}
    {/each}
</div>

<style>
    .context-menu {
        z-index: 100;
        min-width: 200px;
        max-height: 70vh;
        overflow-y: auto;
        background: var(--bg3);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
        animation: menuFadeIn 0.12s cubic-bezier(0.2, 0, 0, 1);
    }

    @keyframes menuFadeIn {
        from {
            opacity: 0;
            transform: scale(0.97);
        }
    }

    .item {
        background: transparent;
        color: var(--color2);
        font-family: var(--font-ui);
        font-size: 13px;
        text-align: left;
        white-space: nowrap;
        cursor: default;
    }

    .item:hover:not(:disabled),
    .item:focus-visible {
        background: var(--bg5);
        color: var(--color1);
        outline: none;
    }

    .item:disabled {
        opacity: 0.4;
    }

    .item.danger:hover {
        color: #ff8a80;
    }

    .heading {
        font-size: 11px;
        color: var(--color3);
        text-transform: uppercase;
        letter-spacing: 0.06em;
    }

    .separator {
        background: var(--bg5);
    }
</style>
