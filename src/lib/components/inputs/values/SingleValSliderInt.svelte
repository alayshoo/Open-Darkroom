<!-- lib/components/SingleValSliderInt.svelte -->
<script lang="ts">

    import "$lib/styles/palette.css"

    let {
        value = $bindable(50),
        defaultValue = 50,
        min = 0,
        max = 100,
        step = 1,
        gradientStartColor = "#4d4d4d",
        gradientEndColor = "#e6e6e6",
        stepAnimation = true,
        springy = true,
        onCommit = () => {},
        onInteractionStart = () => {},
    }: {
        value?: number;
        defaultValue?: number;
        min?: number;
        max?: number;
        step?: number;
        gradientStartColor?: string;
        gradientEndColor?: string;
        /** Play a subtle scale-pulse animation each time the value steps */
        stepAnimation?: boolean;
        /** Magnetic detent: pointer resists leaving integer positions and snaps past the midpoint */
        springy?: boolean;
        onCommit?: () => void;
        onInteractionStart?: () => void;
    } = $props();

    function snapToStep(v: number): number {
        return Math.round((v - min) / step) * step + min;
    }

    /**
     * Continuous cursor-mapped position (never snapped).
     * Drives the visual pointer when springy=true.
     */
    let rawValue = $state(value);

    // When value changes externally (keyboard, double-click, external bind),
    // keep rawValue in sync so the visual position doesn't drift.
    $effect(() => {
        if (!isDragging) rawValue = value;
    });

    let bar_percentage = $derived(((value - min) / (max - min)) * 100);

    /**
     * Magnetic distortion: maps a linear 0..1 position within a step interval
     * to a compressed position that resists leaving the integer endpoints.
     * Uses a sinusoidal blend so derivative → 0 at the endpoints (full attraction)
     * and peaks above 1 near the midpoint (snap-over acceleration).
     *
     * strength=0 → linear (no effect), strength=1 → full sine distortion.
     */
    const MAGNET_STRENGTH = 0.72;

    function magneticT(t: number): number {
        const sine = 0.5 * Math.sin(Math.PI * (t - 0.5)) + 0.5;
        return (1 - MAGNET_STRENGTH) * t + MAGNET_STRENGTH * sine;
    }

    function rawToDisplayPct(raw: number): number {
        const clamped = Math.max(min, Math.min(max, raw));
        if (!springy) return ((clamped - min) / (max - min)) * 100;

        const stepsFromMin = (clamped - min) / step;
        const floored      = Math.floor(stepsFromMin);
        const t            = stepsFromMin - floored;          // 0..1 within step
        const tDisplay     = magneticT(t);
        const displayVal   = floored * step + min + tDisplay * step;
        return ((displayVal - min) / (max - min)) * 100;
    }

    let displayPct = $derived(rawToDisplayPct(rawValue));

    let isDragging = $state(false);
    let isStepping = $state(false);
    let stepTimeout: ReturnType<typeof setTimeout> | null = null;
    let prevValue = value;

    $effect(() => {
        const current = value;
        if (stepAnimation && current !== prevValue) {
            isStepping = true;
            if (stepTimeout) clearTimeout(stepTimeout);
            stepTimeout = setTimeout(() => { isStepping = false; }, 160);
        }
        prevValue = current;
    });

    function handlePointerDown(e: PointerEvent) {

        isDragging = true;
        onInteractionStart();

        const slider = e.currentTarget as HTMLDivElement;
        const rect = slider.getBoundingClientRect();
        const pointerCenter = (bar_percentage / 100) * rect.width;
        const clickX = e.clientX - rect.left;
        const threshold = 20;
        const initialRawValue = rawValue;
        const initialPointerX = e.clientX;

        function setRaw(raw: number) {
            rawValue = Math.max(min, Math.min(max, raw));
            value    = snapToStep(rawValue);
        }

        function updateValue(clientX: number) {
            const percent = (clientX - rect.left) / rect.width;
            setRaw(percent * (max - min) + min);
        }

        function updateValueRelative(clientX: number) {
            const delta      = clientX - initialPointerX;
            const deltaValue = (delta / rect.width) * (max - min);
            setRaw(initialRawValue + deltaValue);
        }

        const isNearPointer = Math.abs(clickX - pointerCenter) <= threshold;

        if (!isNearPointer) {
            updateValue(e.clientX);
        }

        function onPointerMove(moveEvent: PointerEvent) {
            if (isNearPointer) {
                updateValueRelative(moveEvent.clientX);
            } else {
                updateValue(moveEvent.clientX);
            }
        }

        function onPointerUp() {
            isDragging = false;
            // Snap rawValue clean so there's no leftover sub-integer offset
            rawValue = value;
            document.removeEventListener('pointermove', onPointerMove);
            document.removeEventListener('pointerup', onPointerUp);
            onCommit();
        }

        document.addEventListener('pointermove', onPointerMove);
        document.addEventListener('pointerup', onPointerUp);
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
            e.preventDefault();
            onInteractionStart();
            value = Math.min(max, value + step);
            onCommit();
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
            e.preventDefault();
            onInteractionStart();
            value = Math.max(min, value - step);
            onCommit();
        }
    }

    function interpolateColor(color1: string, color2: string, t: number): string {
        const c1 = parseInt(color1.slice(1), 16);
        const c2 = parseInt(color2.slice(1), 16);

        const r1 = (c1 >> 16) & 255, g1 = (c1 >> 8) & 255, b1 = c1 & 255;
        const r2 = (c2 >> 16) & 255, g2 = (c2 >> 8) & 255, b2 = c2 & 255;

        const r = Math.round(r1 + (r2 - r1) * t);
        const g = Math.round(g1 + (g2 - g1) * t);
        const b = Math.round(b1 + (b2 - b1) * t);

        return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, '0')}`;
    }

    // Bar track follows the snapped value; pointer follows the distorted display
    const midColor = $derived(interpolateColor(gradientStartColor, gradientEndColor, bar_percentage / 100));

    function handleDoubleClick() {
        value    = snapToStep(defaultValue);
        rawValue = value;
        onCommit();
    }

</script>

<div
    class="slider-container"
    role="slider"
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-orientation="horizontal"
    tabindex="0"
    onpointerdown={handlePointerDown}
    onkeydown={handleKeyDown}
    ondblclick={handleDoubleClick}
>
    <div class="slider" style="--color-start: {gradientStartColor}; --color-mid: {midColor}; --color-end: {gradientEndColor};">
        <div class="left-bar"  style="width: calc({bar_percentage}% - 12px);"></div>
        <div class="right-bar" style="left: calc({bar_percentage}% + 12px);"></div>
    </div>
    <div
        class="pointer"
        class:dragging={isDragging}
        class:stepping={isStepping}
        class:springy={springy}
        style="left: calc({(springy ? bar_percentage : displayPct) / 100} * (100% - 4px) + 2px)"
    ></div>
</div>

<style>
    .slider-container {
        position: relative;
        height: 22px;
        user-select: none;
        justify-content: center;
        filter: drop-shadow(0 0 12px #00000040);
    }

    .slider {
        position: absolute;
        top: 50%;
        left: 0;
        right: 0;
        height: 4px;
        display: flex;
        flex-direction: row;
        gap: 24px;
        background: transparent;
        background-size: 100% 100%;
        transform: translateY(-50%);
    }

    .left-bar {
        position: absolute;
        left: 0;
        height: 4px;
        border-radius: 1px;
        background: linear-gradient(to right, var(--color-start), var(--color-mid));
    }

    .right-bar {
        position: absolute;
        right: 0;
        height: 4px;
        border-radius: 1px;
        background: linear-gradient(to right, var(--color-mid), var(--color-end));
    }

    .pointer {
        position: absolute;
        top: 50%;
        width: 4px;
        height: 16px;
        background: var(--color1);
        border-radius: 5px;
        transform: translate(-50%, -50%);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
        transition:
            width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
            height 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
            background 0.6s cubic-bezier(0.2, 0.0, 0, 1.0);
    }

    .pointer.springy {
        transition:
            left 0.12s cubic-bezier(0.34, 1.56, 0.64, 1),
            width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
            height 0.2s cubic-bezier(0.2, 0.0, 0, 1.0),
            background 0.6s cubic-bezier(0.2, 0.0, 0, 1.0);
    }

    .pointer::before {    /**Expands the bounding box of pointer*/
        content: '';
        position: absolute;
        top: 50%;
        left: 50%;
        width: 20px;
        height: 20px;
        transform: translate(-50%, -50%);
    }

    .pointer:hover {
        width: 5px;
        height: 18px;
    }

    .pointer.dragging {
        width: 12px;
        height: 22px;
    }

</style>
