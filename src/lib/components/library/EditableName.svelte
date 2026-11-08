<!-- lib/components/library/EditableName.svelte -->
<script lang="ts">
    // A name shown as text that turns into a field on double click.
    // Enter or leaving the field keeps the new name; Escape drops it.
    let {
        value,
        label,
        onrename,
        editing = $bindable(false),
    }: { value: string; label: string; onrename: (name: string) => void; editing?: boolean } = $props();

    function begin(node: HTMLInputElement) {
        node.focus();
        node.select();
    }

    function finish(input: HTMLInputElement, keep: boolean) {
        if (!editing) return;
        editing = false;
        const name = input.value.trim();
        if (keep && name && name !== value) onrename(name);
    }

    function onkeydown(e: KeyboardEvent) {
        const input = e.currentTarget as HTMLInputElement;
        if (e.key === "Enter") finish(input, true);
        else if (e.key === "Escape") finish(input, false);
        e.stopPropagation();
    }
</script>

{#if editing}
    <input
        class="name-field"
        aria-label={label}
        {value}
        use:begin
        {onkeydown}
        onblur={(e) => finish(e.currentTarget, true)}
    />
{:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="name-text" title="Double-click to rename" ondblclick={() => (editing = true)}>{value}</span>
{/if}

<style>
    .name-text {
        display: block;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .name-field {
        width: calc(100% + 12px);
        margin: -3px -6px;
        padding: 2px 5px;
        border: 1px solid var(--color4);
        border-radius: 7px;
        background: rgba(0, 0, 0, 0.3);
        color: inherit;
        font: inherit;
        line-height: inherit;
        outline: none;
    }
</style>
