<!-- lib/components/editor/LighterRoomPages.svelte -->
<svelte:options runes />
<script lang="ts">
    import { slide } from "svelte/transition";

    import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
    import HslBandGroup from "$lib/components/inputs/values/HslBandGroup.svelte";
    import CurveEditor from "$lib/components/inputs/CurveEditor.svelte";
    import RevertButton from "$lib/components/inputs/revertButton.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import ControlsPage from "./ControlsPage.svelte";
    import UnsharpMask from "./UnsharpMask.svelte";
</script>

<!-- Sharpness Panel -->
<ControlsPage>
    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
        <span class="section-title pb-1.5">Sharpness</span>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.clarity}
            defaultValue={0}
            name="Clarity"
            unit="%"
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "clarity",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.texture}
            defaultValue={0}
            name="Texture"
            unit="%"
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "texture",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
    </div>
    <UnsharpMask />
</ControlsPage>
<!-- Light & Color Panel -->
<ControlsPage>
    {#if editor.edit.isRgb}
        <div
            class="slider-section flex flex-col mt-4 ml-4 mr-3"
            transition:slide={{ duration: 200 }}
        >
            <span class="section-title pb-1.5"
                >White Balance</span
            >
            <SingleValSliderGroup
                bind:value={editor.edit.sliders.wbTemp}
                defaultValue={5500}
                name="Temperature"
                unit="K"
                min={2700}
                max={12000}
                decimalPlaces={0}
                sliderStep={100}
                dragStep={1}
                keyboardStep={10}
                scale="reciprocal"
                centerValue={5500}
                gradientStartColor="#3EAFFF"
                gradientEndColor="#FD8B00"
                onCommit={(oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key: "wbTemp",
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></SingleValSliderGroup>
            <SingleValSliderGroup
                bind:value={editor.edit.sliders.wbTint}
                defaultValue={0}
                name="Tint"
                unit="%"
                min={-100}
                max={100}
                decimalPlaces={1}
                sliderStep={1}
                keyboardStep={0.1}
                gradientStartColor="#64FF76"
                gradientEndColor="#FF66F7"
                onCommit={(oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key: "wbTint",
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></SingleValSliderGroup>
        </div>
    {/if}
    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
        <span class="section-title pb-1.5">
            {editor.edit.isRgb ? "Light & Colour" : "Light"}
        </span>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.exposure}
            defaultValue={0}
            name="Exposure"
            unit="EV"
            min={-5}
            max={5}
            sliderStep={0.1}
            dragStep={0.0005}
            keyboardStep={0.01}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "exposure",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.contrast}
            defaultValue={0}
            name="Contrast"
            unit="%"
            min={-100}
            max={100}
            decimalPlaces={1}
            sliderStep={1}
            keyboardStep={0.1}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "contrast",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.brightness}
            defaultValue={0}
            name="Brightness"
            unit=""
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "brightness",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        {#if editor.edit.isRgb}
            <div
                transition:slide={{ duration: 200 }}
            >
                <div
                    class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"
                ></div>
                <SingleValSliderGroup
                    bind:value={editor.edit.sliders.vibrance}
                    defaultValue={0}
                    name="Vibrance"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                    gradientEndColor="#FF0509"
                    onCommit={(oldVal, newVal) =>
                        editor.commit({
                            type: "slider",
                            key: "vibrance",
                            oldValue: oldVal,
                            newValue: newVal,
                        })}
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={editor.edit.sliders.saturation}
                    defaultValue={0}
                    name="Saturation"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                    gradientEndColor="#FF0509"
                    onCommit={(oldVal, newVal) =>
                        editor.commit({
                            type: "slider",
                            key: "saturation",
                            oldValue: oldVal,
                            newValue: newVal,
                        })}
                ></SingleValSliderGroup>
            </div>
        {/if}
        <div class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"></div>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.highlights}
            defaultValue={0}
            name="Highlights"
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            gradientStartColor={"#606060"}
            gradientEndColor={"#afafaf"}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "highlights",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.shadows}
            defaultValue={0}
            name="Shadows"
            unit=""
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            gradientStartColor={"#303030"}
            gradientEndColor={"#3f3f3f"}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "shadows",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.whites}
            defaultValue={0}
            name="Whites"
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            gradientStartColor={"#b0b0b0"}
            gradientEndColor={"#ffffff"}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "whites",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
        <SingleValSliderGroup
            bind:value={editor.edit.sliders.blacks}
            defaultValue={0}
            name="Blacks"
            min={-100}
            max={100}
            decimalPlaces={1}
            dragStep={0.1}
            sliderStep={1}
            keyboardStep={0.1}
            gradientStartColor={"#000000"}
            gradientEndColor={"#161616"}
            onCommit={(oldVal, newVal) =>
                editor.commit({
                    type: "slider",
                    key: "blacks",
                    oldValue: oldVal,
                    newValue: newVal,
                })}
        ></SingleValSliderGroup>
    </div>
</ControlsPage>
<!-- Curves, HSL & Colour Mixer Panel -->
<ControlsPage>
    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
        <CurveEditor
            bind:curves={editor.edit.curves}
            bind:channel={editor.curveChannel}
            bind:histogramCanvas={editor.curveHistogramCanvas}
            isRgb={editor.edit.isRgb}
            onCommit={(oldCurves, newCurves) =>
                editor.commit({ type: "curves", oldCurves, newCurves })}
        >
            {#snippet title()}
                <span class="section-title">Curves</span>
            {/snippet}
        </CurveEditor>
    </div>
    {#if editor.edit.isRgb}
        <div
            class="slider-section flex flex-col mt-4 ml-4 mr-3"
            transition:slide={{ duration: 200 }}
        >
            <span class="section-title pb-1.5">HSL</span>
            <SingleValSliderGroup
                bind:value={editor.edit.sliders.hue}
                defaultValue={0}
                name="Hue"
                unit="º"
                min={-180}
                max={180}
                decimalPlaces={1}
                sliderStep={1}
                keyboardStep={0.1}
                gradientEndColor="#FF0509"
                onCommit={(oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key: "hue",
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></SingleValSliderGroup>
            <SingleValSliderGroup
                bind:value={editor.edit.sliders.vibrance}
                defaultValue={0}
                name="Vibrance"
                unit="%"
                min={-100}
                max={100}
                decimalPlaces={1}
                sliderStep={1}
                keyboardStep={0.1}
                gradientEndColor="#FF0509"
                onCommit={(oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key: "vibrance",
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></SingleValSliderGroup>
            <SingleValSliderGroup
                bind:value={editor.edit.sliders.saturation}
                defaultValue={0}
                name="Saturation"
                unit="%"
                min={-100}
                max={100}
                decimalPlaces={1}
                sliderStep={1}
                keyboardStep={0.1}
                gradientEndColor="#FF0509"
                onCommit={(oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key: "saturation",
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></SingleValSliderGroup>
        </div>
        <div
            class="slider-section flex flex-col mt-4 ml-4 mr-3"
            transition:slide={{ duration: 200 }}
        >
            <div class="flex flex-row items-center justify-between pb-1.5 pr-1.5">
                <span class="section-title">Colour Mixer</span>
                <RevertButton
                    visible={!editor.isColourMixerIdentity}
                    label="Reset Colour Mixer"
                    onrevert={editor.resetColourMixer}
                />
            </div>
            <HslBandGroup
                bind:sliders={editor.edit.sliders}
                onCommit={(key, oldVal, newVal) =>
                    editor.commit({
                        type: "slider",
                        key,
                        oldValue: oldVal,
                        newValue: newVal,
                    })}
            ></HslBandGroup>
        </div>
    {/if}
</ControlsPage>

<style>
    .section-title {
        font-family: var(--font-ui);
        font-size: 18px;
        font-weight: 600;
        color: var(--color1);
    }

    .sliders-separator {
        background: var(--color5);
    }
</style>
