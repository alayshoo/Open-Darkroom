<!-- lib/components/editor/UnsharpMask.svelte -->
<script lang="ts">
    import Loupe from "$lib/components/outputs/Loupe.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
    import { editor } from "$lib/editor/editor.svelte";
</script>

<!-- Shared by both modes, so the two sets of controls stay one and the same. -->
<div class="slider-section flex flex-col mt-4 ml-4 mr-3">
    <span class="section-title pb-1.5"
        >Unsharp Mask</span
    >
    <Loupe />
    <SingleValSliderGroup
        bind:value={editor.edit.sliders.usmAmount}
        defaultValue={0}
        name="Amount"
        unit="%"
        min={0}
        max={300}
        allowOverflow
        hardMin={-100}
        decimalPlaces={0}
        dragStep={1}
        sliderStep={1}
        keyboardStep={1}
        onCommit={(oldVal, newVal) =>
            editor.commit({
                type: "slider",
                key: "usmAmount",
                oldValue: oldVal,
                newValue: newVal,
            })}
    ></SingleValSliderGroup>
    <SingleValSliderGroup
        bind:value={editor.edit.sliders.usmRadius}
        defaultValue={1}
        name="Radius"
        unit="px"
        min={0.5}
        max={10}
        decimalPlaces={2}
        dragStep={0.01}
        sliderStep={0.1}
        keyboardStep={0.01}
        onCommit={(oldVal, newVal) =>
            editor.commit({
                type: "slider",
                key: "usmRadius",
                oldValue: oldVal,
                newValue: newVal,
            })}
    ></SingleValSliderGroup>
    <SingleValSliderGroup
        bind:value={editor.edit.sliders.usmLumaThreshold}
        defaultValue={0}
        name="Luma Threshold"
        unit="%"
        min={0}
        max={100}
        decimalPlaces={1}
        dragStep={0.1}
        sliderStep={1}
        keyboardStep={0.1}
        gradientStartColor="#000000"
        gradientEndColor="#ffffff"
        onCommit={(oldVal, newVal) =>
            editor.commit({
                type: "slider",
                key: "usmLumaThreshold",
                oldValue: oldVal,
                newValue: newVal,
            })}
    ></SingleValSliderGroup>
    <SingleValSliderGroup
        bind:value={editor.edit.sliders.usmDetailThreshold}
        defaultValue={0}
        name="Detail Threshold"
        unit="%"
        min={0}
        max={100}
        decimalPlaces={1}
        dragStep={0.1}
        sliderStep={1}
        keyboardStep={0.1}
        onCommit={(oldVal, newVal) =>
            editor.commit({
                type: "slider",
                key: "usmDetailThreshold",
                oldValue: oldVal,
                newValue: newVal,
            })}
    ></SingleValSliderGroup>
</div>
