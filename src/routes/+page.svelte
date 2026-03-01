<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { slide, blur } from "svelte/transition";
    import { tick } from "svelte";

    import TitleBar from "$lib/components/window/TitleBar.svelte";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import ColorModeToggle from "$lib/components/inputs/colorModeToggle.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
    import TripleValSliderGroup from "$lib/components/inputs/values/TripleValSliderGroup.svelte";
    import DoubleValSliderGroup from "$lib/components/inputs/values/DoubleValSliderGroup.svelte";
    import ExportButton from "$lib/components/inputs/exportButton.svelte";
    import ModeToggle from "$lib/components/inputs/modeToggle.svelte";

    import { SLIDER_DEFAULTS, COLOR_KEYS, BW_TARGETS, type Sliders, } from "$lib/config/slidersConfig";
    import { mapSlidersToAdjustments } from "$lib/types/adjustments";

    import { animateObject } from "$lib/utils/animateObject";

    import { history } from "$lib/history/history.svelte";
    import { applyAction, undoAction, type StateAccessors, } from "$lib/history/historyDispatch";
    import type { Action } from "$lib/types/historyActions";

    import { createKeydownHandler } from "$lib/config/keyboardShortcuts";



    // ===== Control Modes =====

    let isDarkroom = $state(false);
    let outgoingPager: "normal" | "darkroom" | null = $state(null);
    let outgoingDots: "normal" | "darkroom" | null = $state(null);

    $effect(() => {
        document.documentElement.dataset.theme = isDarkroom ? "darkroom" : "";
    });

    function handleModeToggle() {
        isDarkroom = !isDarkroom;
    }

    // ===== Control Pages =====

    let activePageNormal = $state(1);
    let activePageDarkroom = $state(0);
    let pagerElNormal: HTMLDivElement | null = $state(null);
    let pagerElDarkroom: HTMLDivElement | null = $state(null);

    function handleNormalPagerScroll() {
        if (!pagerElNormal) return;
        activePageNormal = Math.round(
            pagerElNormal.scrollLeft / pagerElNormal.clientWidth,
        );
    }

    function handleDarkroomPagerScroll() {
        if (!pagerElDarkroom) return;
        activePageDarkroom = Math.round(
            pagerElDarkroom.scrollLeft / pagerElDarkroom.clientWidth,
        );
    }

    function handlePagerWheel(e: WheelEvent) {
        // only intercept vertical intent
        if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;

        const pages = pagerEl?.querySelectorAll<HTMLElement>(".controls-page");
        const page = pages?.[activePage];
        if (!page) return;

        // Only if that page can actually scroll vertically
        if (page.scrollHeight <= page.clientHeight) return;

        e.preventDefault();
        page.scrollBy({ top: e.deltaY, behavior: "auto" });
    }

    function scrollToPage(index: number) {
        const el = isDarkroom ? pagerElDarkroom : pagerElNormal;
        el?.scrollTo({
            left: index * el.clientWidth,
            behavior: "smooth",
        });
    }

    let lastRestoredNormal: HTMLDivElement | null = null;
    let lastRestoredDarkroom: HTMLDivElement | null = null;
    $effect(() => {
        if (pagerElNormal && !isDarkroom && lastRestoredNormal !== pagerElNormal) 
        {
            lastRestoredNormal = pagerElNormal;
            pagerElNormal.scrollLeft =
                activePageNormal * pagerElNormal.clientWidth;
        }
        if (isDarkroom) lastRestoredNormal = null;

        if (pagerElDarkroom && isDarkroom && lastRestoredDarkroom !== pagerElDarkroom) 
        {
            lastRestoredDarkroom = pagerElDarkroom;
            pagerElDarkroom.scrollLeft = 0;
        }
        if (!isDarkroom) lastRestoredDarkroom = null;
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

    let activePage = $derived(
        isDarkroom ? activePageDarkroom : activePageNormal,
    );
    let pageCount = $derived(isDarkroom ? 1 : isRgb ? 4 : 3);
    let pagerEl = $derived<HTMLDivElement | null>(
        isDarkroom ? pagerElDarkroom : pagerElNormal,
    );

    async function handleColorModeToggle(isBw: boolean) {
        if (isBw) {
            savedColorValues = {};
            for (const k of COLOR_KEYS) savedColorValues[k] = sliders[k];
            animateObject(sliders, BW_TARGETS);
            if (activePageNormal > 2) {
                await tick(); // wait for the {#if isRgb} block to unmount
                scrollToPage(2);
            }
        } else if (savedColorValues) {
            animateObject(sliders, savedColorValues);
            savedColorValues = null;
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
        changeMode: handleModeToggle,
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
                <div class="controls-pager-wrapper">
                    {#if !isDarkroom}
                        <div
                            class="controls-pager"
                            class:outgoing={outgoingPager === "normal"}
                            bind:this={pagerElNormal}
                            onscroll={handleNormalPagerScroll}
                            onwheel={handlePagerWheel}
                            onwheelcapture={handlePagerWheel}
                            onoutrostart={() => (outgoingPager = "normal")}
                            onoutroend={() => (outgoingPager = null)}
                            transition:blur={{ duration: 500, amount: 30 }}
                        >
                            <!-- Sharpness Panel -->
                            <div class="controls-page">
                                <div class="slider-section">
                                    <span class="section-title">Sharpness</span>
                                    <div
                                        style="display:flex; width: 100%; aspect-ratio: 1.5 / 1; background: black; border-radius: 6px;"
                                    ></div>
                                    <div class="sliders-separator"></div>
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
                                    <span class="section-title"
                                        >Unsharp Mask</span
                                    >
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
                                        <span class="section-title"
                                            >White Balance</span
                                        >
                                        <SingleValSliderGroup
                                            bind:value={sliders.wbTemp}
                                            defaultValue={5600}
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
                                        defaultValue={0}
                                        name="Exposure"
                                        unit="EV"
                                        min={-5}
                                        max={5}
                                        sliderStep={0.1}
                                        dragStep={0.0005}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
                                        name="Brightness"
                                        unit=""
                                        min={-10}
                                        max={10}
                                        decimalPlaces={2}
                                        dragStep={0.001}
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
                                        <div
                                            transition:slide={{ duration: 200 }}
                                        >
                                            <div
                                                class="sliders-separator"
                                            ></div>
                                            <SingleValSliderGroup
                                                bind:value={sliders.vibrance}
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
                                                    commit({
                                                        type: "slider",
                                                        key: "vibrance",
                                                        oldValue: oldVal,
                                                        newValue: newVal,
                                                    })}
                                            ></SingleValSliderGroup>
                                            <SingleValSliderGroup
                                                bind:value={sliders.saturation}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                        defaultValue={0}
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
                                            defaultValue={0}
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
                                                commit({
                                                    type: "slider",
                                                    key: "vibrance",
                                                    oldValue: oldVal,
                                                    newValue: newVal,
                                                })}
                                        ></SingleValSliderGroup>
                                        <SingleValSliderGroup
                                            bind:value={sliders.saturation}
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
                    {/if}
                    {#if isDarkroom}
                        <div
                            class="controls-pager"
                            class:outgoing={outgoingPager === "darkroom"}
                            bind:this={pagerElDarkroom}
                            onscroll={handleDarkroomPagerScroll}
                            onwheel={handlePagerWheel}
                            onwheelcapture={handlePagerWheel}
                            onoutrostart={() => (outgoingPager = "darkroom")}
                            onoutroend={() => (outgoingPager = null)}
                            transition:blur={{ duration: 500, amount: 30 }}
                        >
                            <div class="controls-page">
                                <div class="slider-section">
                                    <TripleValSliderGroup
                                        name="Red Input"
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                    ></TripleValSliderGroup>
                                    <TripleValSliderGroup
                                        name="Green Input"
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                    ></TripleValSliderGroup>
                                    <TripleValSliderGroup
                                        name="Blue Input"
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                    ></TripleValSliderGroup>
                                    <DoubleValSliderGroup
                                        name="RGB Output"
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                    ></DoubleValSliderGroup>
                                </div>
                            </div>
                        </div>
                    {/if}
                </div>
            </div>
        </div>
        <div class="side-panel-footer">
            <div style="position:absolute; left: 10px;">
                <ExportButton></ExportButton>
            </div>
            <div class="page-dots-wrapper">
                {#if !isDarkroom}
                    <div
                        class="page-dots"
                        class:outgoing={outgoingDots === "normal"}
                        onoutrostart={() => (outgoingDots = "normal")}
                        onoutroend={() => (outgoingDots = null)}
                        transition:blur={{ duration: 500, amount: 30 }}
                    >
                        {#each Array(isRgb ? 4 : 3) as _, i}
                            <button
                                class="dot"
                                class:active={activePageNormal === i}
                                onclick={() => scrollToPage(i)}
                                aria-label="Go to page {i + 1}"
                                aria-current={activePageNormal === i ? "step" : undefined}
                            ></button>
                        {/each}
                    </div>
                {/if}
                {#if isDarkroom}
                    <div
                        class="page-dots"
                        class:outgoing={outgoingDots === "darkroom"}
                        onoutrostart={() => (outgoingDots = "darkroom")}
                        onoutroend={() => (outgoingDots = null)}
                        transition:blur={{ duration: 500, amount: 30 }}
                    >
                        <button
                            class="dot active"
                            aria-label="Darkroom page"
                            aria-current="step"
                        ></button>
                    </div>
                {/if}
            </div>
            <div style="position: absolute; right: 10px;">
                <ModeToggle bind:isDarkroom onModeToggle={handleModeToggle}
                ></ModeToggle>
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
        background: #262626;
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
        box-shadow: inset 0 0 var(--panelInnerShadowSpread) var(--panelInnerShadow);
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

    .controls-pager-wrapper {
        position: relative;
        flex: 1;
        min-height: 0;
        overflow: hidden;
    }

    .controls-pager {
        position: absolute;
        inset: 0;
        display: flex;
        overflow-x: auto;
        overflow-y: hidden;
        scroll-snap-type: x mandatory;
        scrollbar-width: none;
        z-index: 1;
        backface-visibility: hidden;
    }
    .controls-pager.outgoing {
        z-index: 2;
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
        /* Top and bottom fades via mask - no overlays, so inner shadow on side-panel stays visible */
        mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            black 20px,
            black calc(100% - 24px),
            transparent 100%
        );
        -webkit-mask-image: linear-gradient(
            to bottom,
            transparent 0%,
            black 20px,
            black calc(100% - 24px),
            transparent 100%
        );
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

    .side-panel-footer {
        display: flex;
        position: relative;
        flex-shrink: 0;
        justify-content: center;
        align-items: center;
        height: 48px;
        border-radius: 12px;
        background: var(--bg4);
        box-shadow: inset 0 0 var(--panelInnerShadowSpread) var(--panelInnerShadow);
    }

    .page-dots-wrapper {
        position: relative;
        display: flex;
        justify-content: center;
        align-items: center;
        min-width: 72px; /* fit up to 4 dots + gaps */
    }

    .page-dots-wrapper .page-dots {
        position: absolute;
        left: 50%;
        top: 50%;
        transform: translate(-50%, -50%);
        z-index: 1;
        backface-visibility: hidden;
        display: flex;
        justify-content: center;
        align-items: center;
        gap: 12px;
    }
    .page-dots-wrapper .page-dots.outgoing {
        z-index: 2;
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
