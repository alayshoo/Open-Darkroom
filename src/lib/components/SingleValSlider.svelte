<!-- lib/components/SingleValSlider.svelte -->
<script lang="ts">
    
    let {
        value = $bindable(50),
        min = 0,
        max = 100,
        step = 1,
        gradientStartColor = "#4d4d4d",
        gradientEndColor = "#e6e6e6"
    }: {
        value?: number;
        min?: number;
        max?: number;
        step?: number;
        gradientStartColor?: string;
        gradientEndColor?: string;
    } = $props();

    const defaultValue = value; // capture default automatically

    let bar_percentage = $derived(((value - min) / (max - min)) * 100);

    let isDragging = $state(false)
    
    function handlePointerDown(e: PointerEvent) {

        isDragging = true;

        const slider = e.currentTarget as HTMLDivElement;
        const rect = slider.getBoundingClientRect();
        const pointerCenter = (bar_percentage / 100) * rect.width;
        const clickX = e.clientX - rect.left;
        const threshold = 20; // pixels from pointer center
        const initialValue = value;
        const initialPointerX = e.clientX;
        
        function updateValue(clientX: number) {
            const percent = (clientX - rect.left) / rect.width;
            value = Math.max(min, Math.min(max, 
                percent * (max - min) + min
            ));
        }

        function updateValueRelative(clientX: number) {
            const delta = clientX - initialPointerX;
            const deltaPercent = (delta / rect.width) * (max - min);
            value = Math.max(min, Math.min(max, initialValue + deltaPercent));
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
        }

        document.addEventListener('pointermove', onPointerMove);
        document.addEventListener('pointerup', onPointerUp);
    }

    function handleKeyDown(e: KeyboardEvent) {
        
        if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
            e.preventDefault();
            value = Math.min(max, value + step);
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
            e.preventDefault();
            value = Math.max(min, value - step);
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
    <div class="slider" style= " --color-start: {gradientStartColor}; --color-mid: {midColor}; --color-end: {gradientEndColor};">
        <div class="left-bar" style="width: calc({bar_percentage}% - 12px);"></div>
        <div class="right-bar" style="left: calc({bar_percentage}% + 12px);"></div>
    </div>
    <div class="pointer" class:dragging={isDragging} style="left: calc({bar_percentage / 100} * (100% - 4px) + 2px)"></div>
    
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
        transition: width 0.2s cubic-bezier(0.2, 0.0, 0, 1.0), height 0.2s cubic-bezier(0.2, 0.0, 0, 1.0);
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