<!-- lib/components/PrecisionVal.svelte -->
<script lang="ts">
    import { tick } from "svelte";

    let {
        value = $bindable(0),
        unit = "",
        min = -Infinity,
        max = Infinity,
        step = 1,
        keyboardStep = step * 10,
        decimalPlaces = 2,
    }: {
        value: number;
        unit?: string;
        min?: number;
        max?: number;
        step?: number;
        keyboardStep?: number;
        decimalPlaces?: number;
    } = $props();

    const defaultValue = value; // capture default automatically

    let isEditing = $state(false);
    let isDragging = $state(false);
    let hasDragged = $state(false);
    let editValue = $state("");
    let inputElement: HTMLInputElement = $state.raw()!;
    let spanElement: HTMLSpanElement = $state.raw()!;

    function selectValueOnly() {
        inputElement?.focus();
        const end = unit ? editValue.length - unit.length : editValue.length;
        inputElement?.setSelectionRange(0, Math.max(0, end));
    }

    function clamp(val: number) {
        return Math.max(min, Math.min(max, val));
    }

    function formatValue(val: number) {
        return val.toFixed(decimalPlaces) + (unit ? unit : "");
    }

    async function enterEditMode() {
        editValue = formatValue(value);
        isEditing = true;
        await tick();
        selectValueOnly();
    }

    function handlePointerDown(e: PointerEvent) {
        if (isEditing) return;

        isDragging = true;
        hasDragged = false;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        document.body.style.cursor = "none";
    }

    function handlePointerMove(e: PointerEvent) {
        if (!isDragging) return;
        hasDragged = true;
        value = clamp(value + e.movementX * step);
    }

    function handlePointerUp(e: PointerEvent) {
        if (!isDragging) return;

        isDragging = false;
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
        document.body.style.cursor = "";
    }

    function handleClick() {
        if (!hasDragged && !isDragging) {
            enterEditMode();
        }
        hasDragged = false;
    }

    function handleBlur() {
        isEditing = false;
        const stripped = unit
            ? editValue.replace(new RegExp(`\\s*${unit}\\s*$`), "")
            : editValue;
        const parsed = parseFloat(stripped);
        if (!isNaN(parsed)) {
            value = clamp(parsed);
        }
    }

    function handleKeydownInput(e: KeyboardEvent) {
        e.stopPropagation(); // prevent bubbling to div handler

        if (e.key === "Enter" || e.key === "Escape") {
            e.preventDefault();
            inputElement.blur();
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            value = clamp(value + keyboardStep);
            editValue = formatValue(value);
            requestAnimationFrame(() => selectValueOnly());
        } else if (e.key === "ArrowDown") {
            e.preventDefault();
            value = clamp(value - keyboardStep);
            editValue = formatValue(value);
            requestAnimationFrame(() => selectValueOnly());
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if (isEditing) return;

        if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            enterEditMode();
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            value = clamp(value + keyboardStep);
        } else if (e.key === "ArrowDown") {
            e.preventDefault();
            value = clamp(value - keyboardStep);
        } else if (e.key === "Backspace") {
            e.preventDefault();
            value = defaultValue;
        }
    }

    let displayValue = $derived(formatValue(value));

    let isHovered = $state(false); // track whether component is hovered

    function handleWindowKeydown(e: KeyboardEvent) {
        if (isHovered && !isEditing && e.key === "Backspace") {
            e.preventDefault();
            e.stopImmediatePropagation();
            (document.activeElement as HTMLElement)?.blur();
            value = defaultValue;
        }
    }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div
    class="value-input"
    class:editing={isEditing}
    role="textbox"
    tabindex="0"
    onpointerleave={() => (isHovered = false)}
    onpointerenter={() => (isHovered = true)}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
    onclick={handleClick}
    onkeydown={handleKeydown}
>
    <span bind:this={spanElement} class="value" class:hidden={isEditing}>
        {displayValue}
    </span>
    <input
        bind:this={inputElement}
        bind:value={editValue}
        type="text"
        onblur={handleBlur}
        onkeydown={handleKeydownInput}
        class="value value-input-box"
        class:hidden={!isEditing}
        style="width: {spanElement?.offsetWidth}px;"
    />
</div>

<style>
    .value-input {
        position: relative;
        display: inline-flex;
        align-items: center;
        padding: 0.125rem 0.35rem;
        border: none;
        border-radius: 6px;
        cursor: ew-resize;
        user-select: none;

        background: transparent;
        color: #e6e6e6;

        font-family: "Figtree", sans-serif;
        font-weight: 600;

        transition: background 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .value-input:hover {
        background: #101010;
    }
    .value-input.editing {
        background: #101010;
    }
    .value-input::selection {
        background: #333333; /* your custom highlight color */
        color: #e6e6e6; /* text color when highlighted */
    }

    .value-input-box {
        background: #101010;
    }

    .value,
    input.value {
        text-align: right;
    }

    input.value {
        border: none;
        outline: none;
        background: transparent;
        font: inherit;
        color: #e6e6e6;
        padding: 0;
        min-width: 0;
    }

    .hidden {
        visibility: hidden;
        position: absolute;
    }
</style>
