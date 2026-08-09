<!-- lib/components/MenuButton.svelte -->
<script lang="ts">
    import "$lib/styles/palette.css";
    import { onMount, onDestroy } from "svelte";
    import { blur } from "svelte/transition";
    import { menuBar } from "./menuBar.svelte";

    type MenuItem =
        | {
              label: string;
              shortcut?: string;
              action?: () => void;
              disabled?: false;
              separator?: false;
          }
        | {
              separator: true;
          };

    let {
        label,
        items = [],
    }: {
        label: string;
        items: MenuItem[];
    } = $props();

    /* Which button is open lives on the bar, not here — see menuBar.svelte.ts. */
    const id = $props.id();
    let isOpen = $derived(menuBar.isOpen(id));
    let buttonEl: HTMLButtonElement;
    let menuEl: HTMLDivElement | null = $state(null);

    function close() {
        menuBar.close();
    }

    function handleItemClick(item: MenuItem) {
        if ("separator" in item && item.separator) return;
        item.action?.();
        close();
    }

    function handleKeydown(e: KeyboardEvent) {
        close();
        buttonEl.blur();
    }

    function handleClickOutside(e: MouseEvent) {
        if (
            isOpen &&
            buttonEl &&
            !buttonEl.contains(e.target as Node) &&
            menuEl &&
            !menuEl.contains(e.target as Node)
        ) {
            close();
        }
    }

    onMount(() => {
        document.addEventListener("mousedown", handleClickOutside);
        document.addEventListener("keydown", handleKeydown);
    });

    onDestroy(() => {
        document.removeEventListener("mousedown", handleClickOutside);
        document.removeEventListener("keydown", handleKeydown);
    });
</script>

<div class="menu-button-wrapper relative">
    <button
        bind:this={buttonEl}
        class="menu-button relative inline-flex justify-center items-center h-5.5 px-1.5 py-0 rounded-[6px] border-0 cursor-app"
        class:active={isOpen}
        onclick={() => menuBar.toggle(id)}
        onmouseenter={() => menuBar.hover(id)}
    >
        {label}
    </button>

    {#if isOpen}
        <div
            bind:this={menuEl}
            class="menu-dropdown absolute p-1 rounded-[10px] border border-bg5 z-100 flex flex-col"
            class:instant={menuBar.switching}
        >
            {#each items as item}
                {#if "separator" in item && item.separator}
                    <div class="menu-separator h-0.25 mx-2 my-1"></div>
                {:else}
                    <button
                        class="menu-item flex items-center justify-between w-full h-7.5 px-2.5 py-0 border-0 rounded-[8px] cursor-app"
                        onclick={() => handleItemClick(item)}
                    >
                        <span class="menu-item-label flex-1">{item.label}</span>
                        {#if item.shortcut}
                            <span class="menu-item-shortcut ml-6"
                                >{item.shortcut}</span
                            >
                        {/if}
                    </button>
                {/if}
            {/each}
        </div>
    {/if}
</div>

<style>
    .menu-button {
        background: transparent;
        color: var(--color2);
        font-family: "Figtree", sans-serif;
        font-size: 14px;
        transition:
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            border-radius 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .menu-button:hover,
    .menu-button.active {
        background: var(--bg3);
    }

    /* ── Dropdown ── */
    .menu-dropdown {
        top: calc(100% + 6px);
        left: 0;
        min-width: 210px;
        background: var(--bg3);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);

        animation: menuFadeIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
    }

    /* Hover-switching between siblings keeps the bar open the whole time, so
       the incoming panel appears in place rather than fading in again. */
    .menu-dropdown.instant {
        animation: none;
    }

    @keyframes menuFadeIn {
        from {
            opacity: 0;
            transform: translateY(-4px) scale(0.97);
        }
        to {
            opacity: 1;
            transform: translateY(0) scale(1);
        }
    }

    /* ── Menu items ── */
    .menu-item {
        background: transparent;
        color: var(--color2);
        font-family: "Figtree", sans-serif;
        font-size: 13px;
        text-align: left;
        transition: background 0.12s ease;
    }

    .menu-item:hover {
        background: var(--bg5);
        color: var(--color1);
    }

    .menu-item-shortcut {
        font-size: 11px;
        opacity: 0.5;
        white-space: nowrap;
    }

    /* ── Separator ── */
    .menu-separator {
        background: var(--bg5);
    }
</style>
