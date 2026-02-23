<!-- lib/components/PrecisionVal.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { tick } from "svelte";

    let {
        value = $bindable(0),
        defaultValue = 0,
        unit = "",
        min = -Infinity,
        max = Infinity,
        step = 1,
        keyboardStep = step * 10,
        decimalPlaces = 2,
        onCommit = () => {},
        onInteractionStart = () => {},
    }: {
        value: number;
        defaultValue: number;
        unit?: string;
        min?: number;
        max?: number;
        step?: number;
        keyboardStep?: number;
        decimalPlaces?: number;
        onCommit?: () => void;
        onInteractionStart?: () => void;
    } = $props();


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
        onInteractionStart();
        editValue = formatValue(value);
        isEditing = true;
        await tick();
        selectValueOnly();
    }

    function handlePointerDown(e: PointerEvent) {
        onInteractionStart();
        if (isEditing) return;

        isDragging = true;
        hasDragged = false;
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        document.body.style.cursor = "none";
    }

    function handlePointerMove(e: PointerEvent) {
        if (!isDragging) return;

        if (Math.abs(e.movementX) > 100) return;

        hasDragged = true;
        value = clamp(value + e.movementX * step);

        const dpr = window.devicePixelRatio || 1;
        void invoke("wrap_cursor", { x: e.clientX * dpr, y: e.clientY * dpr });
    }

    function handlePointerUp(e: PointerEvent) {
        if (!isDragging) return;
        void invoke("grab_cursor", { grab: false });

        isDragging = false;
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
        document.body.style.cursor = "";
        onCommit();
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
        onCommit();
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
            onCommit();
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
    <span bind:this={spanElement} class="value" class:invisible={isEditing}>
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
    />
</div>

<style>
    .value-input {
        position: relative;
        display: inline-flex;
        align-items: center;
        padding: 0.35rem 0.35rem;
        border: none;
        border-radius: 4px;
        cursor: ew-resize;
        user-select: none;

        background: transparent;
        color: var(--color1);

        font-family: "Figtree", sans-serif;
        font-weight: 500;
        font-size: 14px;
        line-height: 100%;

        transition: background 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .value-input:hover {
        background: var(--bg2);
    }
    .value-input.editing {
        background: var(--bg2);
    }
    .value-input::selection {
        background: #333333; /* your custom highlight color */
        color: var(--color1); /* text color when highlighted */
    }

    .value-input-box {
        background: var(--bg2);
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
        color: var(--color1);
        padding: 0;
        min-width: 0;
        position: absolute;
        inset: 0;
        padding: 0.35rem;
        padding-top: 0.3rem;
    }

    .invisible {
        visibility: hidden;
    }

    .hidden {
        visibility: hidden;
        position: absolute;
        pointer-events: none;
    }
</style>
