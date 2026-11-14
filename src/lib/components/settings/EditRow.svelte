<!-- lib/components/settings/EditRow.svelte -->
<script lang="ts">
    // One record in a settings list: a summary line that opens into its fields.
    import type { Snippet } from "svelte";

    let {
        title,
        summary,
        open = false,
        ontoggle,
        ondelete,
        deleteLabel,
        icon,
        children,
    }: {
        title: string;
        summary: string;
        open?: boolean;
        ontoggle: () => void;
        ondelete: () => void;
        deleteLabel: string;
        icon?: Snippet;
        children: Snippet;
    } = $props();

    let armed = $state(false);

    function remove() {
        if (!armed) {
            armed = true;
            return;
        }
        ondelete();
    }
</script>

<div class="record flex flex-col" class:open>
    <button class="line flex items-center" aria-expanded={open} onclick={ontoggle}>
        {@render icon?.()}
        <span class="title">{title}</span>
        <span class="summary flex-1">{summary}</span>
        <svg class="chevron" width="14" height="14" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M9 6l6 6-6 6" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
    </button>

    {#if open}
        <div class="fields grid" class:indented={icon}>
            {@render children()}
            <div class="footer wide flex items-center justify-end">
                <button class="text-button danger" class:armed onclick={remove} onblur={() => (armed = false)}>
                    {armed ? "Click again to delete" : deleteLabel}
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

    .title {
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
        padding: 4px 12px 14px 12px;
    }

    .fields.indented {
        padding-left: 44px;
    }

    .fields :global(.field) {
        display: flex;
        flex-direction: column;
        gap: 4px;
        min-width: 0;
        font-size: 12px;
        color: var(--color2);
    }

    .fields :global(.wide) {
        grid-column: 1 / -1;
    }

    .fields :global(input:not([type="color"])) {
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

    .fields :global(input:not([type="color"]):focus) {
        border-color: rgba(255, 255, 255, 0.28);
    }

    .fields :global(input::placeholder) {
        color: var(--color4);
    }

    .fields :global(.choice) {
        display: flex;
        gap: 4px;
        padding: 3px;
        border-radius: 8px;
        background: rgba(0, 0, 0, 0.3);
    }

    .fields :global(.choice button) {
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

    .fields :global(.choice button.on) {
        background: var(--bg5);
        color: var(--color1);
    }

    .fields :global(.text-button) {
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

    .fields :global(.text-button:hover) {
        background: var(--bg5);
        color: var(--color1);
    }

    .danger.armed,
    .danger:hover {
        color: #ff8a80;
    }
</style>
