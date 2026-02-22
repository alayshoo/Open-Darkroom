<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { slide } from "svelte/transition";
    import { tick } from "svelte";

    import TitleBar from "$lib/components/window/TitleBar.svelte";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import ColorModeToggle from "$lib/components/inputs/colorModeToggle.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/SingleValSliderGroup.svelte";
    import ExportButton from "$lib/components/inputs/exportButton.svelte";

    import {
        SLIDER_DEFAULTS,
        COLOR_KEYS,
        BW_TARGETS,
        type Sliders,
    } from "$lib/config/slidersConfig";
    import { mapSlidersToAdjustments } from "$lib/types/adjustments";

    import { animateObject } from "$lib/utils/animateObject";

    import { history } from "$lib/history/history.svelte";
    import {
        applyAction,
        undoAction,
        type StateAccessors,
    } from "$lib/history/historyDispatch";
    import type { Action } from "$lib/types/historyActions";

    import { createKeydownHandler } from "$lib/config/keyboardShortcuts";

    // ===== Control Pages =====

    let pageCount = $state(4);
    let activePage = $state(0);
    let pagerEl: HTMLDivElement;

    function handlePagerScroll() {
        if (!pagerEl) return;
        const index = Math.round(pagerEl.scrollLeft / pagerEl.clientWidth);
        activePage = index;
    }

    function scrollToPage(index: number) {
        pagerEl?.scrollTo({
            left: index * pagerEl.clientWidth,
            behavior: "smooth",
        });
    }

    $effect(() => {
        if (pagerEl) {
            pagerEl.scrollLeft = pagerEl.clientWidth;
            activePage = 1;
        }
    });

    function movePage(right?: boolean) {
        const next = right
            ? Math.min(activePage + 1, pageCount - 1)
            : Math.max(activePage - 1, 0);
        scrollToPage(next);
    }

    // ===== Variables =====

    let sliders: Sliders = $state({
        wbTemp: 5600,
        wbTint: 0,
        exposure: 0,
        contrast: 0,
        brightness: 0,
        saturation: 0,
        vibrance: 0,
    });

    let adjustments = $derived(mapSlidersToAdjustments(sliders));

    // ===== B&W toggle logic =====
    let isRgb = $state(true);
    let savedColorValues: Partial<typeof sliders> | null = null;

    async function handleColorModeToggle(isBw: boolean) {
        if (isBw) {
            savedColorValues = {};
            for (const k of COLOR_KEYS) savedColorValues[k] = sliders[k];
            animateObject(sliders, BW_TARGETS);
            pageCount = 3;
            if (activePage > 2) {
                await tick(); // wait for the {#if isRgb} block to unmount
                scrollToPage(2);
            }
        } else if (savedColorValues) {
            animateObject(sliders, savedColorValues);
            savedColorValues = null;
            pageCount = 4;
        }
    }

    // ===== History =====
    const stateAccessors: StateAccessors = {
        getSlider: (key) => sliders[key as keyof Sliders],
        setSlider: (key, val) => {
            (sliders as any)[key] = val;
        },
    };

    function commit(action: Action) {
        history.push(action);
    }
    function undo() {
        const a = history.undo();
        if (a) undoAction(a, stateAccessors);
    }
    function redo() {
        const a = history.redo();
        if (a) applyAction(a, stateAccessors);
    }

    const handleKeydown = createKeydownHandler({
        undo: undo,
        redo: redo,
        movePage: movePage,
    });
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
            <div class="quick-actions">
                <ColorModeToggle bind:isRgb onToggle={handleColorModeToggle} />
            </div>
            <div class="controls-section">
                <div class="controls-top-gradient"></div>
                <div
                    class="controls-pager"
                    bind:this={pagerEl}
                    onscroll={handlePagerScroll}
                >
                    <!-- Sharpness Panel -->
                    <div class="controls-page">
                        <div class="slider-section">
                            <span class="section-title">Sharpness</span>
                            <SingleValSliderGroup
                                bind:value={sliders.exposure}
                                name="Clarity"
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
                                name="Texture"
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
                        </div>
                        <div class="slider-section">
                            <span class="section-title">Unsharp Mask</span>
                            <SingleValSliderGroup
                                bind:value={sliders.exposure}
                                name="Amount"
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
                                name="Radius"
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
                                bind:value={sliders.exposure}
                                name="Luma Threshold"
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
                                name="Detail Threshold"
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
                        </div>
                        <div style="display: flex; height:30px;"></div>
                    </div>
                    <!-- Light & Color Panel -->
                    <div class="controls-page">
                        {#if isRgb}
                            <div
                                class="slider-section"
                                transition:slide={{ duration: 200 }}
                            >
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
                        {/if}
                        <div class="slider-section">
                            <span class="section-title">
                                {isRgb ? "Light & Colour" : "Light"}
                            </span>
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
                            {#if isRgb}
                                <div transition:slide={{ duration: 200 }}>
                                    <div class="sliders-separator"></div>
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
                            {/if}
                            <div class="sliders-separator"></div>
                            <SingleValSliderGroup
                                bind:value={sliders.contrast}
                                name="Highlights"
                                unit="%"
                                min={-100}
                                max={100}
                                decimalPlaces={1}
                                sliderStep={1}
                                keyboardStep={0.1}
                                gradientStartColor={"#606060"}
                                gradientEndColor={"#afafaf"}
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
                                name="Shadows"
                                unit=""
                                min={-10}
                                max={10}
                                decimalPlaces={2}
                                sliderStep={10}
                                keyboardStep={1}
                                gradientStartColor={"#303030"}
                                gradientEndColor={"#3f3f3f"}
                                onCommit={(oldVal, newVal) =>
                                    commit({
                                        type: "slider",
                                        key: "brightness",
                                        oldValue: oldVal,
                                        newValue: newVal,
                                    })}
                            ></SingleValSliderGroup>
                            <SingleValSliderGroup
                                bind:value={sliders.contrast}
                                name="Whites"
                                unit="%"
                                min={-100}
                                max={100}
                                decimalPlaces={1}
                                sliderStep={1}
                                keyboardStep={0.1}
                                gradientStartColor={"#b0b0b0"}
                                gradientEndColor={"#ffffff"}
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
                                name="Blacks"
                                unit=""
                                min={-10}
                                max={10}
                                decimalPlaces={2}
                                sliderStep={10}
                                keyboardStep={1}
                                gradientStartColor={"#000000"}
                                gradientEndColor={"#161616"}
                                onCommit={(oldVal, newVal) =>
                                    commit({
                                        type: "slider",
                                        key: "brightness",
                                        oldValue: oldVal,
                                        newValue: newVal,
                                    })}
                            ></SingleValSliderGroup>
                        </div>
                        <div style="display: flex; height:30px;"></div>
                    </div>
                    <!-- Curves Panel -->
                    <div class="controls-page">
                        <div class="slider-section">
                            <span class="section-title">Curves</span>
                            <div
                                style="display:flex; width: 100%; aspect-ratio: 1 / 1; background: black; border-radius: 6px;"
                            ></div>
                            <div class="sliders-separator"></div>
                            <SingleValSliderGroup
                                bind:value={sliders.contrast}
                                name="Highlights"
                                unit="%"
                                min={-100}
                                max={100}
                                decimalPlaces={1}
                                sliderStep={1}
                                keyboardStep={0.1}
                                gradientStartColor={"#606060"}
                                gradientEndColor={"#afafaf"}
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
                                name="Shadows"
                                unit=""
                                min={-10}
                                max={10}
                                decimalPlaces={2}
                                sliderStep={10}
                                keyboardStep={1}
                                gradientStartColor={"#303030"}
                                gradientEndColor={"#3f3f3f"}
                                onCommit={(oldVal, newVal) =>
                                    commit({
                                        type: "slider",
                                        key: "brightness",
                                        oldValue: oldVal,
                                        newValue: newVal,
                                    })}
                            ></SingleValSliderGroup>
                            <SingleValSliderGroup
                                bind:value={sliders.contrast}
                                name="Whites"
                                unit="%"
                                min={-100}
                                max={100}
                                decimalPlaces={1}
                                sliderStep={1}
                                keyboardStep={0.1}
                                gradientStartColor={"#b0b0b0"}
                                gradientEndColor={"#ffffff"}
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
                                name="Blacks"
                                unit=""
                                min={-10}
                                max={10}
                                decimalPlaces={2}
                                sliderStep={10}
                                keyboardStep={1}
                                gradientStartColor={"#000000"}
                                gradientEndColor={"#161616"}
                                onCommit={(oldVal, newVal) =>
                                    commit({
                                        type: "slider",
                                        key: "brightness",
                                        oldValue: oldVal,
                                        newValue: newVal,
                                    })}
                            ></SingleValSliderGroup>
                        </div>
                        <div style="display: flex; height:30px;"></div>
                    </div>
                    <!-- HSL Panel -->
                    {#if isRgb}
                        <div class="controls-page">
                            <div class="slider-section">
                                <span class="section-title">HSL</span>
                                <SingleValSliderGroup
                                    bind:value={sliders.vibrance}
                                    name="Hue"
                                    unit="º"
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
                                <div class="sliders-separator"></div>
                            </div>
                        </div>
                        <div style="display: flex; height:30px;"></div>
                    {/if}
                </div>
                <div class="controls-bottom-gradient"></div>
            </div>
        </div>
        <div class="side-panel-footer">
            <div class="page-dots">
                <div style= "position:absolute; left: 10px;"><ExportButton></ExportButton></div>
                {#each Array(pageCount) as _, i}
                    <button
                        class="dot"
                        class:active={activePage === i}
                        onclick={() => scrollToPage(i)}
                        aria-label="Go to page {i + 1} of {pageCount}"
                        aria-current={activePage === i ? "step" : undefined}
                    ></button>
                {/each}
            </div>
        </div>
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
        border-radius: 8px;
    }

    .side-bar {
        display: flex;
        flex-direction: column;
        width: 23%;
        gap: 12px;
        min-width: 300px;
    }

    .histogram-container {
        margin: 5px;
    }

    .quick-actions {
        display: flex;
        flex-direction: row;
        margin-left: 12px;
        margin-top: 6px;
    }

    .side-panel {
        background: var(--bg4);
        flex: 1;
        border-radius: 12px;
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
        flex: 1;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        position: relative;
    }

    .controls-pager {
        display: flex;
        flex: 1;
        overflow-x: auto;
        overflow-y: hidden;
        scroll-snap-type: x mandatory;
        scrollbar-width: none;
    }
    .controls-pager::-webkit-scrollbar {
        display: none;
    }

    .controls-page {
        min-width: 100%;
        max-width: 100%;
        overflow-y: auto;
        scrollbar-width: none;
        scroll-snap-align: start;
    }
    .controls-page::-webkit-scrollbar {
        display: none;
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

    .sliders-separator {
        height: 2px;
        border-radius: 1px;
        margin-top: 12px;
        margin-bottom: 8px;
        margin-right: 6px;
        background: var(--color5);
    }

    .controls-top-gradient {
        position: absolute;
        width: 100%;
        height: 20px;
        top: 0;
        border-radius: 6px;
        background: linear-gradient(0, transparent, var(--bg4), var(--bg4));
        pointer-events: none;
        z-index: 999;
    }

    .controls-bottom-gradient {
        position: absolute;
        width: 100%;
        height: 20px;
        bottom: 0;
        border-radius: 6px;
        background: linear-gradient(0, var(--bg4), var(--bg4), transparent);
        pointer-events: none;
        z-index: 999;
    }

    .side-panel-footer {
        display: flex;
        position: relative;
        flex-shrink: 0;
        justify-content: center;
        align-items: center;
        height: 48px;
        border-radius: 12px;
        background: var(--bg5);
    }

    .page-dots {
        display: flex;
        justify-content: center;
        align-items: center;
        gap: 12px;
    }

    .dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        border: none;
        padding: 0;
        background: var(--color5);
        cursor: pointer;
        transition:
            background 0.2s,
            transform 0.2s;
    }
    .dot.active {
        background: var(--color1);
        transform: scale(1.3);
    }
</style>
