<!-- lib/components/MenuButton.svelte -->
<script lang="ts">
    import "$lib/styles/palette.css"
    import { onMount, onDestroy } from "svelte";
    import { blur } from "svelte/transition";

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

    let isOpen = $state(false);
    let buttonEl: HTMLButtonElement;
    let menuEl: HTMLDivElement | null = $state(null);

    function toggle() {
        isOpen = !isOpen;
    }

    function close() {
        isOpen = false;
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

<div class="menu-button-wrapper">
    <button
        bind:this={buttonEl}
        class="menu-button"
        class:active={isOpen}
        onclick={toggle}
    >
        {label}
    </button>

    {#if isOpen}
        <div bind:this={menuEl} class="menu-dropdown">
            {#each items as item}
                {#if "separator" in item && item.separator}
                    <div class="menu-separator"></div>
                {:else}
                    <button
                        class="menu-item"
                        onclick={() => handleItemClick(item)}
                    >
                        <span class="menu-item-label">{item.label}</span>
                        {#if item.shortcut}
                            <span class="menu-item-shortcut"
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
    .menu-button-wrapper {
        position: relative;
    }

    .menu-button {
        position: relative;
        display: inline-flex;
        justify-content: center;
        align-items: center;
        height: 28px;
        padding: 0 10px;
        border-radius: 6px;
        border: none;
        background: var(--bg3);
        color: var(--color2);
        font-family: "Figtree", sans-serif;
        font-size: 14px;
        cursor:
            url("/cursors/cursor32x32.png") 7 7,
            auto;
        transition:
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            border-radius 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .menu-button:hover,
    .menu-button.active {
        background: var(--bg5);
        border-radius: 8px;
    }

    /* ── Dropdown ── */
    .menu-dropdown {
        position: absolute;
        top: calc(100% + 6px);
        left: 0;
        min-width: 210px;
        padding: 4px;
        border-radius: 10px;
        background: var(--bg3);
        border: 1px solid var(--bg5);
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
        z-index: 100;
        display: flex;
        flex-direction: column;

        animation: menuFadeIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
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
        display: flex;
        align-items: center;
        justify-content: space-between;
        width: 100%;
        height: 30px;
        padding: 0 10px;
        border: none;
        border-radius: 8px;
        background: transparent;
        color: var(--color2);
        font-family: "Figtree", sans-serif;
        font-size: 13px;
        text-align: left;
        cursor:
            url("/cursors/cursor32x32.png") 7 7,
            auto;
        transition: background 0.12s ease;
    }

    .menu-item:hover {
        background: var(--bg5);
        color: var(--color1);
    }

    .menu-item-label {
        flex: 1;
    }

    .menu-item-shortcut {
        margin-left: 24px;
        font-size: 11px;
        opacity: 0.5;
        white-space: nowrap;
    }

    /* ── Separator ── */
    .menu-separator {
        height: 1px;
        margin: 4px 8px;
        background: var(--bg5);
    }
</style>
