<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import TitleBar from "$lib/components/TitleBar.svelte";
    import SingleValSliderGroup from "$lib/components/SingleValSliderGroup.svelte";
    import PreviewImageCanvas from "$lib/components/PreviewImageCanvas.svelte";
  import { mapSlidersToAdjustments } from "$lib/types/adjustments";

    let temperature = $state(5600);
    let tint = $state(0);
    let exposure = $state(0);
    let contrast = $state(0);
    let brightness = $state(0);
    let saturation = $state(0);
    let vibrance = $state(0);

    let adjustments = $derived(
        mapSlidersToAdjustments({
            temperature,
            tint,
            exposure,
            contrast,
            brightness,
            saturation,
            vibrance,
        }),
    );
</script>

<TitleBar></TitleBar>
<div class="container">
    <div class="toolbar"></div>
    <div class="image-panel">
        <div class="preview-container">
            <PreviewImageCanvas {adjustments} imageSrc="/test.jpg"/>
        </div>
    </div>
    <div class="side-bar">
        <div class="side-panel">
            <div class="slider-section">
                <span class="section-title">White Balance</span>
                <SingleValSliderGroup
                    bind:value={temperature}
                    name="Temperature"
                    unit="K"
                    min={1200}
                    max={10000}
                    decimalPlaces={0}
                    sliderStep={100}
                    dragStep={1}
                    keyboardStep={10}
                    gradientStartColor="#FD8B00"
                    gradientEndColor="#3EAFFF"
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={tint}
                    name="Tint"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                    gradientStartColor="#64FF76"
                    gradientEndColor="#FF66F7"
                ></SingleValSliderGroup>
            </div>
            <div class="slider-section">
                <span class="section-title">Light & Colour</span>
                <SingleValSliderGroup
                    bind:value={exposure}
                    name="Exposure"
                    unit="EV"
                    min={-5}
                    max={5}
                    sliderStep={0.1}
                    keyboardStep={0.01}
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={contrast}
                    name="Contrast"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={brightness}
                    name="Brightness"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={vibrance}
                    name="Vibrance"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                    gradientEndColor="#FF0509"
                ></SingleValSliderGroup>
                <SingleValSliderGroup
                    bind:value={saturation}
                    name="Saturation"
                    unit="%"
                    min={-100}
                    max={100}
                    decimalPlaces={1}
                    sliderStep={1}
                    keyboardStep={0.1}
                    gradientEndColor="#FF0509"
                ></SingleValSliderGroup>
            </div>
        </div>
        <div class="side-panel-footer"></div>
    </div>
</div>

<style>
    .container {
        display: flex;
        flex-direction: row;
        gap: 12px;
        position: absolute;
        top: 48px;
        bottom: 12px;
        left: 12px;
        right: 12px;
    }

    .toolbar {
        min-width: 28px;
    }

    .image-panel {
        background: var(--bg5);
        flex: 1;
        border-radius: 6px;
        overflow: hidden;
        display: flex;
        align-items: center;
        justify-content: center;
        min-width: 0; /* prevents flex blowout */
    }

    .preview-container {
        margin: 36px;               /**Change later when there is zoom support*/
        border-radius: 6px;
    }

    .side-bar {
        display: flex;
        flex-direction: column;
        width: 25%;
        gap: 12px;
        width: 300px;
    }

    .side-panel {
        background: var(--bg4);
        flex: 1;
        border-radius: 6px;
        min-height: 0;
        overflow-y: auto;
        scrollbar-width: none;
    }
    .side-panel::-webkit-scrollbar {
        display: none;
    } /* Chrome/Safari/Edge */

    .slider-section {
        display: flex;
        flex-direction: column;
        margin-top: 16px;
        margin-left: 16px;
        margin-right: 12px;
    }

    .section-title {
        font-family: "Figtree", sans-serif;
        font-size: 20px;
        font-weight: 700;
        color: var(--color1);
    }

    .side-panel-footer {
        background: var(--bg4);
        height: 48px;
        border-radius: 6px;
    }
</style>
