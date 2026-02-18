<script lang="ts">
    import SingleValSlider from "./SingleValSlider.svelte";
    import PrecisionVal from "./PrecisionVal.svelte";

    let {
        value = $bindable(50),
        name = "Slider Name",
        unit = "",
        min = 0,
        max = 100,
        decimalPlaces = 2,
        sliderStep = 1,
        dragStep = 0.01,
        keyboardStep = dragStep * 10,
        gradientStartColor = "#4d4d4d",
        gradientEndColor = "#ffffff"
    }: {
        value?: number;
        name?: string;
        unit?: string;
        min?: number;
        max?: number;
        decimalPlaces?: number,
        sliderStep?: number;
        dragStep?: number;
        keyboardStep?: number;
        gradientStartColor?: string;
        gradientEndColor?: string;
    } = $props();

</script>

<div class="container">
    <div class="slider-text-container">
        <span class="slider-name">{name}</span>
        <PrecisionVal 
            bind:value= {value}
            min= {min}
            max= {max}
            unit= {unit}
            step= {dragStep}
            keyboardStep= {keyboardStep}
            decimalPlaces= {decimalPlaces}
            ></PrecisionVal>
    </div>
    <SingleValSlider 
        bind:value= {value}
        min= {min}
        max= {max}
        step= {sliderStep}
        gradientStartColor= {gradientStartColor}
        gradientEndColor= {gradientEndColor}
        ></SingleValSlider>
</div>

<style>
    .container{
        display: flex;
        flex-direction: column;
    }

    .slider-text-container{
        display: flex;
        flex-direction: row;
        align-items: baseline;
        justify-content: space-between;
        width: 100%;
        margin-bottom: -4px;
    }

    .slider-name{
        font-family: 'Figtree', sans-serif;
        font-size: 12px;
        font-weight: 400;
    }
</style>