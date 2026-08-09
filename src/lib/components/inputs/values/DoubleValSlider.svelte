<!-- lib/components/DoubleValSlider.svelte -->
<script lang="ts">
    type ActivePointer = "A" | "B";

    let {
        valueA = $bindable(0),
        valueB = $bindable(100),
        defaultValueA = 0,
        defaultValueB = 100,
        min = 0,
        max = 100,
        step = 1,
        innerColor = "#e6e6e6",
        outerColor = "#4d4d4d",
        onCommit = (_which: ActivePointer) => {},
        onInteractionStart = (_which: ActivePointer) => {},
    }: {
        valueA?: number;
        valueB?: number;
        defaultValueA?: number;
        defaultValueB?: number;
        min?: number;
        max?: number;
        step?: number;
        innerColor?: string;
        outerColor?: string;
        onCommit?: (which: ActivePointer) => void;
        onInteractionStart?: (which: ActivePointer) => void;
    } = $props();

    // Percentage positions along the full track for each pointer (two extremes only)
    const pctA = $derived(((valueA - min) / (max - min)) * 100);
    const pctB = $derived(((valueB - min) / (max - min)) * 100);

    let dragging: ActivePointer | null = $state(null);

    function pointerPos(pct: number) {
        return `calc(${pct / 100} * (100% - 4px) + 2px)`;
    }

    function handlePointerDown(e: PointerEvent) {
        const slider = (e.currentTarget as HTMLElement).closest(".slider-container") as HTMLDivElement;
        const rect = slider.getBoundingClientRect();
        const clickX = e.clientX - rect.left;

        const pointerEl = e.currentTarget as HTMLElement;
        const active = pointerEl.dataset.pointer as ActivePointer;

        dragging = active;
        onInteractionStart(active);

        pointerEl.focus();

        const initialPointerX = e.clientX;
        const activePct = active === "A" ? pctA : pctB;
        const pointerCenter = (activePct / 100) * rect.width;
        const isNearPointer = Math.abs(clickX - pointerCenter) <= 12;

        const initialValueA = valueA,
            initialValueB = valueB;

        function setFromAbsolute(clientX: number) {
            const pct = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
            const val = pct * (max - min) + min;
            if (active === "A") {
                valueA = Math.max(min, Math.min(max, val));
                if (valueA > valueB) valueB = valueA;
            } else {
                valueB = Math.max(min, Math.min(max, val));
                if (valueB < valueA) valueA = valueB;
            }
        }

        function setFromRelative(clientX: number) {
            const delta = clientX - initialPointerX;
            const deltaVal = (delta / rect.width) * (max - min);
            if (active === "A") {
                valueA = Math.max(min, Math.min(max, initialValueA + deltaVal));
                if (valueA > valueB) valueB = valueA;
            } else {
                valueB = Math.max(min, Math.min(max, initialValueB + deltaVal));
                if (valueB < valueA) valueA = valueB;
            }
        }

        if (!isNearPointer) setFromAbsolute(e.clientX);

        function onMove(me: PointerEvent) {
            if (isNearPointer) setFromRelative(me.clientX);
            else setFromAbsolute(me.clientX);
        }

        function onUp() {
            dragging = null;
            document.removeEventListener("pointermove", onMove);
            document.removeEventListener("pointerup", onUp);
            onCommit(active);
        }

        document.addEventListener("pointermove", onMove);
        document.addEventListener("pointerup", onUp);
    }

    function handleKeyDown(e: KeyboardEvent) {
        const pointerEl = e.currentTarget as HTMLElement;
        const active = pointerEl.dataset.pointer as ActivePointer;

        const inc = e.key === "ArrowRight" || e.key === "ArrowUp";
        const dec = e.key === "ArrowLeft" || e.key === "ArrowDown";
        if (!inc && !dec) return;
        e.preventDefault();

        const dir = inc ? 1 : -1;
        onInteractionStart(active);
        if (active === "A") {
            valueA = Math.max(min, Math.min(max, valueA + dir * step));
            if (valueA > valueB) valueB = valueA;
        } else {
            valueB = Math.max(min, Math.min(max, valueB + dir * step));
            if (valueB < valueA) valueA = valueB;
        }
        onCommit(active);
    }

    function handleDoubleClick(e: MouseEvent) {
        const pointerEl = e.currentTarget as HTMLElement;
        const active = pointerEl.dataset.pointer as ActivePointer;

        onInteractionStart(active);
        if (active === "A") valueA = defaultValueA;
        else valueB = defaultValueB;
        onCommit(active);
    }
</script>

<!--
    The outer div is a presentation container (no ARIA role).
    Each pointer is an individually focusable ARIA slider.
-->
<div class="slider-container relative h-5.5">
    <div class="slider absolute h-1" aria-hidden="true">
        <!-- Segment: left of A -->
        <div class="segment absolute h-1 rounded-[1px]" style="left:0; width:calc({pctA}% - 12px); background:{outerColor};"></div>
        <!-- Segment: A to B -->
        <div class="segment absolute h-1 rounded-[1px]" style="left:calc({pctA}% + 12px); width:calc({pctB - pctA}% - 24px); background:{innerColor};"></div>
        <!-- Segment: right of B -->
        <div class="segment absolute h-1 rounded-[1px]" style="left:calc({pctB}% + 12px); right:0; background:{outerColor};"></div>
    </div>

    <!-- Pointer A (left extreme) -->
    <div
        class="pointer absolute w-1 h-4 rounded-[5px]"
        class:dragging={dragging === "A"}
        style="left:{pointerPos(pctA)}"
        role="slider"
        aria-label="Value A"
        aria-valuenow={valueA}
        aria-valuemin={min}
        aria-valuemax={max}
        tabindex="0"
        data-pointer="A"
        onpointerdown={handlePointerDown}
        onkeydown={handleKeyDown}
        ondblclick={handleDoubleClick}
    ></div>

    <!-- Pointer B (right extreme) -->
    <div
        class="pointer absolute w-1 h-4 rounded-[5px]"
        class:dragging={dragging === "B"}
        style="left:{pointerPos(pctB)}"
        role="slider"
        aria-label="Value B"
        aria-valuenow={valueB}
        aria-valuemin={min}
        aria-valuemax={max}
        tabindex="0"
        data-pointer="B"
        onpointerdown={handlePointerDown}
        onkeydown={handleKeyDown}
        ondblclick={handleDoubleClick}
    ></div>
</div>

<style>
    .slider-container {
        user-select: none;
        filter: drop-shadow(0 0 12px #00000040);
    }

    .slider {
        top: 50%;
        left: 0;
        right: 0;
        transform: translateY(-50%);
    }

    .segment {
        min-width: 0;
    }

    .pointer {
        top: 50%;
        background: var(--color1);
        transform: translate(-50%, -50%);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
        transition:
            width 0.2s cubic-bezier(0.2, 0, 0, 1),
            height 0.2s cubic-bezier(0.2, 0, 0, 1),
            background 0.5s cubic-bezier(0.2, 0, 0, 1);
        outline: none;
    }

    .pointer::before {
        content: "";
        position: absolute;
        top: 50%;
        left: 50%;
        width: 20px;
        height: 20px;
        transform: translate(-50%, -50%);
    }

    .pointer:hover,
    .pointer:focus-visible {
        width: 5px;
        height: 18px;
    }

    .pointer.dragging {
        width: 12px;
        height: 22px;
    }
</style>
