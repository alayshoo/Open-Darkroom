<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import TitleBar from "$lib/components/window/TitleBar.svelte";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/SingleValSliderGroup.svelte";

    import { mapSlidersToAdjustments } from "$lib/types/adjustments";

    import { history } from "$lib/history/history.svelte";
    import {
        applyAction,
        undoAction,
        type StateAccessors,
    } from "$lib/history/historyDispatch";
    import type { Action } from "$lib/types/historyActions";

    let sliders = $state({
        wbTemp: 5600,
        wbTint: 0,
        exposure: 0,
        contrast: 0,
        brightness: 0,
        saturation: 0,
        vibrance: 0,
    });

    let adjustments = $derived(mapSlidersToAdjustments(sliders));


    // — State accessors (passed to dispatcher) —
    const stateAccessors: StateAccessors = {
        getSlider: (key) => sliders[key as keyof typeof sliders],
        setSlider: (key, val) => {
            (sliders as any)[key] = val;
        },
    };

    // — Commit an action to history —
    function commit(action: Action) {
        history.push(action);
    }

    function undo() {
        const action = history.undo();
        if (action) undoAction(action, stateAccessors);
    }

    function redo() {
        const action = history.redo();
        if (action) applyAction(action, stateAccessors);
    }

    // — Global keyboard shortcut —
    function handleKeydown(e: KeyboardEvent) {
        if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
            e.preventDefault();
            undo();
        } else if (
            (e.ctrlKey && e.key === "Z") ||
            (e.ctrlKey && e.shiftKey && e.key === "z")
        ) {
            e.preventDefault();
            redo();
        }
    }

</script>
<svelte:window onkeydown={handleKeydown} />

<TitleBar {undo} {redo}></TitleBar>
<div class="container">
    <div class="toolbar"></div>
    <div class="image-panel">
        <div class="preview-container">
            <PreviewImageCanvas {adjustments} imageSrc="/test.jpg" />
        </div>
    </div>
    <div class="side-bar">
        <div class="side-panel">
            <div class="histogram-container">
                <Histogram {adjustments} imageSrc="/test.jpg" />
            </div>
            <div class="controls-section">
                <div class="slider-section">
                    <span class="section-title">White Balance</span>
                    <SingleValSliderGroup
                        bind:value={sliders.wbTemp}
                        name="Temperature"
                        unit="K"
                        min={1200}
                        max={10000}
                        decimalPlaces={0}
                        sliderStep={100}
                        dragStep={1}
                        keyboardStep={10}
                        gradientStartColor="#3EAFFF"
                        gradientEndColor="#FD8B00"
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "wbTemp",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                    <SingleValSliderGroup
                        bind:value={sliders.wbTint}
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
                            commit({
                              type: "slider",
                              key: "wbTint",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                </div>
                <div class="slider-section">
                    <span class="section-title">Light & Colour</span>
                    <SingleValSliderGroup
                        bind:value={sliders.exposure}
                        name="Exposure"
                        unit="EV"
                        min={-5}
                        max={5}
                        sliderStep={0.1}
                        keyboardStep={0.01}
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "exposure",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                    <SingleValSliderGroup
                        bind:value={sliders.contrast}
                        name="Contrast"
                        unit="%"
                        min={-100}
                        max={100}
                        decimalPlaces={1}
                        sliderStep={1}
                        keyboardStep={0.1}
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "contrast",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                    <SingleValSliderGroup
                        bind:value={sliders.brightness}
                        name="Brightness"
                        unit=""
                        min={-10}
                        max={10}
                        decimalPlaces={2}
                        sliderStep={10}
                        keyboardStep={1}
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "brightness",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                    <SingleValSliderGroup
                        bind:value={sliders.vibrance}
                        name="Vibrance"
                        unit="%"
                        min={-100}
                        max={100}
                        decimalPlaces={1}
                        sliderStep={1}
                        keyboardStep={0.1}
                        gradientEndColor="#FF0509"
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "vibrance",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                    <SingleValSliderGroup
                        bind:value={sliders.saturation}
                        name="Saturation"
                        unit="%"
                        min={-100}
                        max={100}
                        decimalPlaces={1}
                        sliderStep={1}
                        keyboardStep={0.1}
                        gradientEndColor="#FF0509"
                        onCommit={(oldVal, newVal) =>
                            commit({
                              type: "slider",
                              key: "saturation",
                              oldValue: oldVal,
                              newValue: newVal,
                            })}
                    ></SingleValSliderGroup>
                </div>
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
        margin: 36px; /**Change later when there is zoom support*/
        border-radius: 6px;
    }

    .side-bar {
        display: flex;
        flex-direction: column;
        width: 23%;
        gap: 12px;
        min-width: 250px;
    }

    .histogram-container {
        margin: 5px;
    }

    .side-panel {
        background: var(--bg4);
        flex: 1;
        border-radius: 6px;
        display: flex;
        flex-direction: column;
        min-height: 0; /* allows it to shrink below content size */
        overflow: hidden;
    }
    .side-panel::-webkit-scrollbar {
        display: none;
    } /* Chrome/Safari/Edge */

    .controls-section {
        min-height: 0;
        overflow-y: auto;
        scrollbar-width: none;
    }

    .slider-section {
        display: flex;
        flex-direction: column;
        margin-top: 16px;
        margin-left: 16px;
        margin-right: 12px;
    }

    .section-title {
        font-family: "Figtree", sans-serif;
        font-size: 18px;
        font-weight: 600;
        color: var(--color1);
        padding-bottom: 6px;
    }

    .side-panel-footer {
        background: var(--bg4);
        height: 48px;
        border-radius: 6px;
    }
</style>
