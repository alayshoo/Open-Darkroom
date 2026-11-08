<!-- lib/components/library/Combobox.svelte -->
<script lang="ts" generics="T extends { id: number }">
    // A field that picks one of the catalog's records: type to narrow the list,
    // or add what was typed as a new one.
    import type { Snippet } from "svelte";

    let {
        items,
        label,
        matches,
        meta,
        icon,
        value,
        ariaLabel,
        placeholder,
        noun,
        onpick,
        onadd,
        onclear,
    }: {
        items: T[];
        label: (item: T) => string;
        matches: (item: T, query: string) => boolean;
        meta?: (item: T) => string;
        /** Drawn before the field and each option; null for the field's own when nothing is picked. */
        icon?: Snippet<[T | null]>;
        /** What the field shows while it is not being typed into. */
        value: string;
        ariaLabel: string;
        placeholder: string;
        /** What a new one is called in the list, as in “Add “x” as a new stock”. */
        noun: string;
        onpick: (item: T) => void;
        onadd: (text: string) => void;
        /** Offered as the last option when the field can be emptied. */
        onclear?: () => void;
    } = $props();

    const listId = $props.id();
    let open = $state(false);
    let query = $state("");
    let active = $state(0);
    let input = $state<HTMLInputElement | null>(null);

    let found = $derived(items.filter((item) => matches(item, query)));
    let exact = $derived(found.some((item) => label(item).toLowerCase() === query.trim().toLowerCase()));
    // The rows the arrow keys walk: each match, then adding, then clearing.
    let options = $derived<("add" | "clear" | T)[]>([
        ...found,
        ...(query.trim() !== "" && !exact ? (["add"] as const) : []),
        ...(onclear ? (["clear"] as const) : []),
    ]);

    function close() {
        open = false;
        query = "";
    }

    function choose(option: "add" | "clear" | T) {
        const text = query.trim();
        close();
        input?.blur();
        if (option === "clear") onclear?.();
        else if (option === "add") onadd(text);
        else onpick(option);
    }

    function onkeydown(e: KeyboardEvent) {
        if (!open) return;
        if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            const step = e.key === "ArrowDown" ? 1 : -1;
            active = (active + step + options.length) % Math.max(options.length, 1);
        } else if (e.key === "Enter" && options[active]) {
            choose(options[active]);
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
        {@render icon?.(null)}
        <input
            bind:this={input}
            role="combobox"
            aria-expanded={open}
            aria-controls={listId}
            aria-label={ariaLabel}
            placeholder={open ? `Search or add a ${noun}` : placeholder}
            value={open ? query : value}
            onfocus={() => {
                query = "";
                active = 0;
                open = true;
            }}
            onblur={() => setTimeout(() => document.activeElement !== input && close(), 120)}
            oninput={(e) => {
                query = e.currentTarget.value;
                active = 0;
            }}
            {onkeydown}
        />
    </div>

    {#if open}
        <div class="options" id={listId} role="listbox">
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
                        <span class="label">Add “{query.trim()}” as a new {noun}</span>
                    {:else if option === "clear"}
                        <span class="label muted">No {noun}</span>
                    {:else}
                        {@render icon?.(option)}
                        <span class="label flex-1">{label(option)}</span>
                        {#if meta?.(option)}<span class="meta">{meta(option)}</span>{/if}
                    {/if}
                </button>
            {/each}
            {#if options.length === 0}
                <p class="empty">Type a name to add a {noun}.</p>
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
        flex-shrink: 0;
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
