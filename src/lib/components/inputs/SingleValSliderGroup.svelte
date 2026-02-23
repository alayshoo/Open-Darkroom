<!-- lib/components/SingleValSliderGroup.svelte -->
<script lang="ts">
    import SingleValSlider from "./SingleValSlider.svelte";
    import PrecisionVal from "./PrecisionVal.svelte";

    let {
        value = $bindable(50),
        defaultValue = 50,
        name = "Slider Name",
        unit = "",
        min = 0,
        max = 100,
        decimalPlaces = 2,
        sliderStep = 1,
        dragStep = 0.01,
        keyboardStep = dragStep * 10,
        gradientStartColor = "#4d4d4d",
        gradientEndColor = "#ffffff",
        onCommit = (_old: number, _new: number) => {},
    }: {
        value?: number;
        defaultValue?: number;
        name?: string;
        unit?: string;
        min?: number;
        max?: number;
        decimalPlaces?: number;
        sliderStep?: number;
        dragStep?: number;
        keyboardStep?: number;
        gradientStartColor?: string;
        gradientEndColor?: string;
        onCommit?: (oldValue: number, newValue: number) => void;
    } = $props();

    let committedValue = $state(value);
    let interacting = $state(false);

    function handleInteractionStart() {
        interacting = true;
        committedValue = value; // snapshot right when interaction begins
    }

    function handleCommit() {
        interacting = false;
        if (committedValue !== value) {
            onCommit(committedValue, value);
        }
        committedValue = value;
    }

</script>

<div class="container">
    <div class="text-container">
        <span class="name">{name}</span>
        <PrecisionVal
            bind:value
            defaultValue={defaultValue}
            min={min}
            max={max}
            unit={unit}
            step={dragStep}
            keyboardStep={keyboardStep}
            decimalPlaces={decimalPlaces}
            onCommit={handleCommit}
            onInteractionStart={handleInteractionStart}
        ></PrecisionVal>
    </div>
    <div class="slider-container">
        <SingleValSlider
            bind:value
            defaultValue={defaultValue}
            min={min}
            max={max}
            step={sliderStep}
            gradientStartColor={gradientStartColor}
            gradientEndColor={gradientEndColor}
            onCommit={handleCommit}
            onInteractionStart={handleInteractionStart}
        ></SingleValSlider>
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
        margin-bottom: -6px;
    }

    .name {
        font-family: "Figtree", sans-serif;
        color: var(--color2);
        font-size: 12px;
        font-weight: 400;
    }

    .slider-container {
        padding-right: 6px;
    }
</style>
