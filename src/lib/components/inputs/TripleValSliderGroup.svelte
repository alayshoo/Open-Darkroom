<!-- lib/components/TripleValSliderGroup.svelte -->
<script lang="ts">
    import TripleValSlider from "./TripleValSlider.svelte";
    import PrecisionVal from "./PrecisionVal.svelte";

    type ActivePointer = "A" | "B" | "C";

    let {
        valueA = $bindable(0),
        valueB = $bindable(1),
        valueC = $bindable(100),
        defaultValueA = 0,
        defaultValueB = 1,
        defaultValueC = 100,
        name = "Slider Name",
        unit = "",
        unitB = "",
        min = 0,
        max = 100,
        minB = 0,
        maxB = 2,
        decimalPlacesA = 1,
        decimalPlacesB = 3,
        decimalPlacesC = 1,
        sliderStep = 1,
        sliderStepB = sliderStep,
        dragStep = 0.01,
        dragStepB = 0.0001,
        keyboardStep = dragStep * 10,
        innerColor = "#e6e6e6",
        outerColor = "#4d4d4d",
        onCommitA = (_old: number, _new: number) => {},
        onCommitB = (_old: number, _new: number) => {},
        onCommitC = (_old: number, _new: number) => {},
    }: {
        valueA?: number;
        valueB?: number;
        valueC?: number;
        defaultValueA?: number;
        defaultValueB?: number;
        defaultValueC?: number;
        name?: string;
        unit?: string;
        unitB?: string;
        min?: number;
        max?: number;
        minB?: number;
        maxB?: number;
        decimalPlacesA?: number;
        decimalPlacesB?: number;
        decimalPlacesC?: number;
        sliderStep?: number;
        sliderStepB?: number;
        dragStep?: number;
        dragStepB?: number;
        keyboardStep?: number;
        innerColor?: string;
        outerColor?: string;
        onCommitA?: (oldValue: number, newValue: number) => void;
        onCommitB?: (oldValue: number, newValue: number) => void;
        onCommitC?: (oldValue: number, newValue: number) => void;
    } = $props();

    // Snapshots taken at interaction start, per value
    let committedA = $state(valueA);
    let committedB = $state(valueB);
    let committedC = $state(valueC);

    // Called by the slider with which pointer was touched
    function handleSliderInteractionStart(which: ActivePointer) {
        if (which === "A") committedA = valueA;
        else if (which === "B") committedB = valueB;
        else committedC = valueC;
    }

    function handleSliderCommit(which: ActivePointer) {
        if (which === "A") {
            if (committedA !== valueA) onCommitA(committedA, valueA);
            committedA = valueA;
        } else if (which === "B") {
            if (committedB !== valueB) onCommitB(committedB, valueB);
            committedB = valueB;
        } else {
            if (committedC !== valueC) onCommitC(committedC, valueC);
            committedC = valueC;
        }
    }

    // PrecisionVal callbacks — each value has its own pair
    function handleInteractionStartA() { committedA = valueA; }
    function handleInteractionStartB() { committedB = valueB; }
    function handleInteractionStartC() { committedC = valueC; }

    function handleCommitA() {
        if (committedA !== valueA) onCommitA(committedA, valueA);
        committedA = valueA;
    }
    function handleCommitB() {
        if (committedB !== valueB) onCommitB(committedB, valueB);
        committedB = valueB;
    }
    function handleCommitC() {
        if (committedC !== valueC) onCommitC(committedC, valueC);
        committedC = valueC;
    }
</script>

<div class="container">
    <div class="text-container">
        <span class="name">{name}</span>
    </div>
    <div class="slider-container">
        <TripleValSlider
            bind:valueA
            bind:valueB
            bind:valueC
            {defaultValueA}
            {defaultValueB}
            {defaultValueC}
            {min}
            {max}
            {minB}
            {maxB}
            step={sliderStep}
            stepB={sliderStepB}
            {innerColor}
            {outerColor}
            onCommit={handleSliderCommit}
            onInteractionStart={handleSliderInteractionStart}
        ></TripleValSlider>
    </div>
    <div class="vals-container">
        <div class="val-wrap val-wrap--left">
            <PrecisionVal
                bind:value={valueA}
                defaultValue={defaultValueA}
                {min}
                {max}
                unit={unit}
                step={dragStep}
                {keyboardStep}
                decimalPlaces={decimalPlacesA}
                onCommit={handleCommitA}
                onInteractionStart={handleInteractionStartA}
            ></PrecisionVal>
        </div>
        <div class="val-wrap val-wrap--center">
            <PrecisionVal
                bind:value={valueB}
                defaultValue={defaultValueB}
                min={minB}
                max={maxB}
                unit={unitB}
                step={dragStepB}
                {keyboardStep}
                decimalPlaces={decimalPlacesB}
                onCommit={handleCommitB}
                onInteractionStart={handleInteractionStartB}
            ></PrecisionVal>
        </div>
        <div class="val-wrap val-wrap--right">
            <PrecisionVal
                bind:value={valueC}
                defaultValue={defaultValueC}
                {min}
                {max}
                unit={unit}
                step={dragStep}
                {keyboardStep}
                decimalPlaces={decimalPlacesC}
                onCommit={handleCommitC}
                onInteractionStart={handleInteractionStartC}
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
    .val-wrap--center {
        justify-content: center;
    }
    .val-wrap--right {
        justify-content: flex-end;
    }

    .slider-container {
        padding-right: 6px;
    }
</style>