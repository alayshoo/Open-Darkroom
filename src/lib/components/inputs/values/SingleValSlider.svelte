<!-- lib/components/SingleValSlider.svelte -->
<script lang="ts">

    import "$lib/styles/palette.css"
    
    let {
        value = $bindable(50),
        defaultValue = 50,
        min = 0,
        max = 100,
        step = 1,
        scale = "linear",
        centerValue = undefined,
        gradientStartColor = "#4d4d4d",
        gradientEndColor = "#e6e6e6",
        onCommit = () => {},
        onInteractionStart = () => {},
    }: {
        value?: number;
        defaultValue?: number;
        min?: number;
        max?: number;
        step?: number;
        scale?: "linear" | "reciprocal";
        centerValue?: number;
        gradientStartColor?: string;
        gradientEndColor?: string;
        onCommit?: () => void;
        onInteractionStart?: () => void;
    } = $props();

    // Track position mapping. `reciprocal` spaces the track by 1/value, for
    // quantities whose effect scales with the reciprocal rather than the value
    // itself. Each half is mapped independently so `centerValue` lands on the
    // midpoint even when the two rails are not equidistant from it.
    const center = $derived(centerValue ?? (min + max) / 2);

    function warp(v: number) {
        return scale === "reciprocal" ? 1 / v : v;
    }

    function valueToFraction(v: number) {
        const wc = warp(center);
        if (v <= center) {
            const w0 = warp(min);
            return w0 === wc ? 0.5 : 0.5 * ((warp(v) - w0) / (wc - w0));
        }
        const w1 = warp(max);
        return w1 === wc ? 0.5 : 0.5 + 0.5 * ((warp(v) - wc) / (w1 - wc));
    }

    function fractionToValue(f: number) {
        const wc = warp(center);
        const w =
            f <= 0.5
                ? warp(min) + (wc - warp(min)) * (f / 0.5)
                : wc + (warp(max) - wc) * ((f - 0.5) / 0.5);
        return scale === "reciprocal" ? 1 / w : w;
    }

    const clampValue = (v: number) => Math.max(min, Math.min(max, v));
    const clampFraction = (f: number) => Math.max(0, Math.min(1, f));

    let bar_percentage = $derived(valueToFraction(value) * 100);

    let isDragging = $state(false)
    
    function handlePointerDown(e: PointerEvent) {

        isDragging = true;
        onInteractionStart();       // record initial value for history

        const slider = e.currentTarget as HTMLDivElement;
        const rect = slider.getBoundingClientRect();
        const pointerCenter = (bar_percentage / 100) * rect.width;
        const clickX = e.clientX - rect.left;
        const threshold = 20; // pixels from pointer center
        const initialFraction = valueToFraction(value);
        const initialPointerX = e.clientX;

        function updateValue(clientX: number) {
            const percent = (clientX - rect.left) / rect.width;
            value = clampValue(fractionToValue(clampFraction(percent)));
        }

        function updateValueRelative(clientX: number) {
            const delta = (clientX - initialPointerX) / rect.width;
            value = clampValue(
                fractionToValue(clampFraction(initialFraction + delta))
            );
        }

        const isNearpointer = Math.abs(clickX - pointerCenter) <= threshold;

        if (!isNearpointer) {
            updateValue(e.clientX);
        }

        function onPointerMove(moveEvent: PointerEvent) {
            if (isNearpointer) {
                updateValueRelative(moveEvent.clientX);
            } else {
                updateValue(moveEvent.clientX);
            }
        }

        function onPointerUp() {
            isDragging = false;
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
            value = clampValue(value + step);
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
            e.preventDefault();
            value = clampValue(value - step);
        }
    }

    function interpolateColor(color1: string, color2: string, t: number): string {
        const c1 = parseInt(color1.slice(1), 16);
        const c2 = parseInt(color2.slice(1), 16);
        
        const r1 = (c1 >> 16) & 255;
        const g1 = (c1 >> 8) & 255;
        const b1 = c1 & 255;
        
        const r2 = (c2 >> 16) & 255;
        const g2 = (c2 >> 8) & 255;
        const b2 = c2 & 255;
        
        const r = Math.round(r1 + (r2 - r1) * t);
        const g = Math.round(g1 + (g2 - g1) * t);
        const b = Math.round(b1 + (b2 - b1) * t);
        
        return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, '0')}`;
    }

    const midColor = $derived(interpolateColor(gradientStartColor, gradientEndColor, bar_percentage/100))

    function handleDoubleClick(){
        value = defaultValue;
        onCommit();
    }

</script>

<div 
    class="slider-container relative h-5.5 justify-center"
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
    <div class="slider absolute h-1 flex flex-row gap-6" style= " --color-start: {gradientStartColor}; --color-mid: {midColor}; --color-end: {gradientEndColor};">
        <div class="left-bar absolute h-1 rounded-[1px]" style="width: calc({bar_percentage}% - 12px);"></div>
        <div class="right-bar absolute h-1 rounded-[1px]" style="left: calc({bar_percentage}% + 12px);"></div>
    </div>
    <div class="pointer absolute w-1 h-4 rounded-[5px]" class:dragging={isDragging} style="left: calc({bar_percentage / 100} * (100% - 4px) + 2px)"></div>
    
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
        background: transparent;
        background-size: 100% 100%;
        transform: translateY(-50%);
    }

    .left-bar {
        left: 0;
        background: linear-gradient(to right, var(--color-start), var(--color-mid));
    }

    .right-bar {
        right: 0;
        background: linear-gradient(to right, var(--color-mid), var(--color-end));
    }

    .pointer {
        top: 50%;
        background: var(--color1);
        transform: translate(-50%, -50%);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
        transition: width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0), height 0.2s cubic-bezier(0.2, 0.0, 0, 1.0), background 0.6s cubic-bezier(0.2, 0.0, 0, 1.0);
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