<!-- lib/components/HslBandGroup.svelte -->
<script lang="ts">
    import SingleValSliderGroup from "./SingleValSliderGroup.svelte";
    import {
        HSL_BANDS,
        hslKey,
        type HslBand,
        type HslChannel,
        type Sliders,
    } from "$lib/types/imgParameters";

    let {
        sliders = $bindable(),
        onCommit = (_key: keyof Sliders, _old: number, _new: number) => {},
    }: {
        sliders: Sliders;
        onCommit?: (key: keyof Sliders, oldValue: number, newValue: number) => void;
    } = $props();

    const SWATCHES: Record<HslBand, string> = {
        red: "#f0443a",
        orange: "#f58a2e",
        yellow: "#f2d33a",
        green: "#4cbf4f",
        aqua: "#3cc6c9",
        blue: "#3f6ff0",
        purple: "#8b4ef0",
        magenta: "#e04bc7",
    };

    const CHANNELS: { id: HslChannel; name: string }[] = [
        { id: "Hue", name: "Hue" },
        { id: "Sat", name: "Saturation" },
        { id: "Lum", name: "Luminance" },
    ];

    let selected = $state<HslBand>("red");

    const index = $derived(HSL_BANDS.indexOf(selected));
    const prevColour = $derived(SWATCHES[HSL_BANDS[(index + 7) % 8]]);
    const nextColour = $derived(SWATCHES[HSL_BANDS[(index + 1) % 8]]);
    const colour = $derived(SWATCHES[selected]);

    // Track ends for each channel: hue runs between the neighbouring bands,
    // saturation out of grey and luminance from dark to light.
    const gradients = $derived<Record<HslChannel, [string, string]>>({
        Hue: [prevColour, nextColour],
        Sat: ["#4d4d4d", colour],
        Lum: [mixHex(colour, 0, 0.6), mixHex(colour, 255, 0.5)],
    });

    function mixHex(hex: string, target: number, t: number) {
        const channels = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
        return "#" + channels
            .map((c) => Math.round(c + (target - c) * t).toString(16).padStart(2, "0"))
            .join("");
    }

    function isAdjusted(band: HslBand) {
        return CHANNELS.some(({ id }) => sliders[hslKey(band, id)] !== 0);
    }
</script>

<div class="flex flex-col">
    <div class="swatches flex flex-row justify-center gap-2 pr-1.5 mb-2.5" role="radiogroup">
        {#each HSL_BANDS as band (band)}
            <button
                class="swatch"
                class:selected={band === selected}
                class:adjusted={isAdjusted(band)}
                style="--swatch: {SWATCHES[band]}"
                role="radio"
                aria-checked={band === selected}
                aria-label={band}
                title={band[0].toUpperCase() + band.slice(1)}
                onclick={() => (selected = band)}
            ></button>
        {/each}
    </div>
    {#each CHANNELS as channel (channel.id)}
        {@const key = hslKey(selected, channel.id)}
        <SingleValSliderGroup
            bind:value={sliders[key]}
            defaultValue={0}
            name={channel.name}
            min={-100}
            max={100}
            decimalPlaces={1}
            sliderStep={1}
            keyboardStep={0.1}
            gradientStartColor={gradients[channel.id][0]}
            gradientEndColor={gradients[channel.id][1]}
            onCommit={(oldVal, newVal) => onCommit(key, oldVal, newVal)}
        ></SingleValSliderGroup>
    {/each}
</div>

<style>
    .swatch {
        position: relative;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: var(--swatch);
        outline: 1.5px solid transparent;
        outline-offset: 2px;
        cursor: pointer;
        transition: outline-color 120ms ease;
    }
    .swatch.selected {
        outline-color: var(--color1);
    }
    .swatch.adjusted::after {
        content: "";
        position: absolute;
        left: 50%;
        bottom: -7px;
        width: 3px;
        height: 3px;
        margin-left: -1.5px;
        border-radius: 50%;
        background: var(--color2);
    }
</style>
