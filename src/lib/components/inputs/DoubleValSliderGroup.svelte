<!-- lib/components/DoubleValSliderGroup.svelte -->
<script lang="ts">
    import DoubleValSlider from "./DoubleValSlider.svelte";
    import PrecisionVal from "./PrecisionVal.svelte";

    type ActivePointer = "A" | "B";

    let {
        valueA = $bindable(0),
        valueB = $bindable(100),
        defaultValueA = 0,
        defaultValueB = 100,
        name = "Slider Name",
        unit = "",
        min = 0,
        max = 100,
        decimalPlacesA = 1,
        decimalPlacesB = 1,
        sliderStep = 1,
        dragStep = 0.01,
        keyboardStep = dragStep * 10,
        innerColor = "#e6e6e6",
        outerColor = "#4d4d4d",
        onCommitA = (_old: number, _new: number) => {},
        onCommitB = (_old: number, _new: number) => {},
    }: {
        valueA?: number;
        valueB?: number;
        defaultValueA?: number;
        defaultValueB?: number;
        name?: string;
        unit?: string;
        min?: number;
        max?: number;
        decimalPlacesA?: number;
        decimalPlacesB?: number;
        sliderStep?: number;
        dragStep?: number;
        keyboardStep?: number;
        innerColor?: string;
        outerColor?: string;
        onCommitA?: (oldValue: number, newValue: number) => void;
        onCommitB?: (oldValue: number, newValue: number) => void;
    } = $props();

    let committedA = $state(valueA);
    let committedB = $state(valueB);

    function handleSliderInteractionStart(which: ActivePointer) {
        if (which === "A") committedA = valueA;
        else committedB = valueB;
    }

    function handleSliderCommit(which: ActivePointer) {
        if (which === "A") {
            if (committedA !== valueA) onCommitA(committedA, valueA);
            committedA = valueA;
        } else {
            if (committedB !== valueB) onCommitB(committedB, valueB);
            committedB = valueB;
        }
    }

    function handleInteractionStartA() { committedA = valueA; }
    function handleInteractionStartB() { committedB = valueB; }

    function handleCommitA() {
        if (committedA !== valueA) onCommitA(committedA, valueA);
        committedA = valueA;
    }
    function handleCommitB() {
        if (committedB !== valueB) onCommitB(committedB, valueB);
        committedB = valueB;
    }
</script>

<div class="container">
    <div class="text-container">
        <span class="name">{name}</span>
    </div>
    <div class="slider-container">
        <DoubleValSlider
            bind:valueA
            bind:valueB
            {defaultValueA}
            {defaultValueB}
            {min}
            {max}
            step={sliderStep}
            {innerColor}
            {outerColor}
            onCommit={handleSliderCommit}
            onInteractionStart={handleSliderInteractionStart}
        ></DoubleValSlider>
    </div>
    <div class="vals-container">
        <div class="val-wrap val-wrap--left">
            <PrecisionVal
                bind:value={valueA}
                defaultValue={defaultValueA}
                {min}
                {max}
                {unit}
                step={dragStep}
                {keyboardStep}
                decimalPlaces={decimalPlacesA}
                onCommit={handleCommitA}
                onInteractionStart={handleInteractionStartA}
            ></PrecisionVal>
        </div>
        <div class="val-wrap val-wrap--right">
            <PrecisionVal
                bind:value={valueB}
                defaultValue={defaultValueB}
                {min}
                {max}
                {unit}
                step={dragStep}
                {keyboardStep}
                decimalPlaces={decimalPlacesB}
                onCommit={handleCommitB}
                onInteractionStart={handleInteractionStartB}
            ></PrecisionVal>
        </div>
    </div>
</div>

<style>
    .container {
        display: flex;
        flex-direction: column;
        margin-top: -3px;
    }

    .text-container {
        display: flex;
        flex-direction: row;
        align-items: baseline;
        justify-content: space-between;
        width: 100%;
    }

    .name {
        font-family: "Figtree", sans-serif;
        color: var(--color2);
        font-size: 12px;
        font-weight: 400;
    }

    .vals-container {
        display: flex;
        flex-direction: row;
        width: 102%;
        margin-left: -6px;
    }

    .val-wrap {
        flex: 1;
        display: flex;
    }

    .val-wrap--left {
        justify-content: flex-start;
    }
    .val-wrap--right {
        justify-content: flex-end;
    }

    .slider-container {
        padding-right: 6px;
    }
</style>
