<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { slide } from "svelte/transition";
    import { cubicInOut } from "svelte/easing";
    import { tick } from "svelte";

    import TitleBar from "$lib/components/window/TitleBar.svelte";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import FrameStatsOverlay from "$lib/components/outputs/FrameStatsOverlay.svelte";
    import { frameStats } from "$lib/gpu/frameStats.svelte";
    import ColorModeToggle from "$lib/components/inputs/colorModeToggle.svelte";
    import InvertToggle from "$lib/components/inputs/invertToggle.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
    import TripleValSliderHistGroup from "$lib/components/inputs/values/TripleValSliderHistGroup.svelte";
    import DoubleValSliderGroup from "$lib/components/inputs/values/DoubleValSliderGroup.svelte";
    import ExportButton from "$lib/components/inputs/exportButton.svelte";
    import ExportSettingsMenu from "$lib/components/inputs/ExportSettingsMenu.svelte";
    import ModeToggle from "$lib/components/inputs/modeToggle.svelte";
    import ExportModal from "$lib/components/ExportModal.svelte";

    import {
        type Sliders,
        defaultSlidersRGB,
        defaultSlidersBW,
        overridesBW,
    } from "$lib/types/imgParameters";

    import { type ImagePayload } from "$lib/types/imagePayload";
    import { openImage } from "$lib/utils/openImage";
    import { exportImage } from "$lib/utils/exportImage";
    import { type ExportSettings, defaultExportSettings } from "$lib/types/exportSettings";

    import { animateObject } from "$lib/utils/animateObject";

    import { history } from "$lib/history/history.svelte";
    import { applyAction, undoAction, type StateAccessors, } from "$lib/history/historyDispatch";
    import type { Action } from "$lib/types/historyActions";

    import { createKeydownHandler } from "$lib/config/keyboardShortcuts";

    // ===== Control Modes =====

    let isDarkroom = $state(false);

    /* The two control pagers sit side by side on an imaginary strip: the normal
       one to the left, the darkroom one to the right. Each always enters from
       and leaves towards its own side, so switching modes reads as the strip
       sliding across rather than as two pagers swapping places. The travel is
       expressed in percent of the pager's own width, which `fly` cannot do
       since it only takes pixels, and .controls-pager-wrapper clips whatever
       hangs outside. */
    function slideX(
        node: Element,
        { direction = 1, duration = 500 }: { direction?: number; duration?: number } = {},
    ) {
        return {
            duration,
            easing: cubicInOut,
            css: (_t: number, u: number) =>
                `transform: translateX(${direction * u * 100}%);`,
        };
    }

    $effect(() => {
        document.documentElement.dataset.theme = isDarkroom ? "darkroom" : "";
    });

    /* Both pager masks are suspended while the strip is in transit. Each
       mask-image forces its own offscreen render surface — .controls-page's
       inside .controls-pager's — and the whole stack lives inside a
       backdrop-filtered panel, so translating it means recompositing two
       nested masked surfaces per frame and re-blurring the panel behind them.
       Dropping the masks for the duration leaves a plain transform.

       Must match slideX's duration; the strip is only mid-flight for that
       long, and re-masking early would pop the fades back mid-slide. */
    const MODE_SLIDE_MS = 500;
    let isModeSliding = $state(false);
    let modeSlideTimer: ReturnType<typeof setTimeout> | null = null;

    function handleModeToggle() {
        isDarkroom = !isDarkroom;

        isModeSliding = true;
        if (modeSlideTimer) clearTimeout(modeSlideTimer);
        modeSlideTimer = setTimeout(() => {
            isModeSliding = false;
            modeSlideTimer = null;
        }, MODE_SLIDE_MS);
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
        if (
            pagerElNormal &&
            !isDarkroom &&
            lastRestoredNormal !== pagerElNormal
        ) {
            lastRestoredNormal = pagerElNormal;
            pagerElNormal.scrollLeft =
                activePageNormal * pagerElNormal.clientWidth;
        }
        if (isDarkroom) lastRestoredNormal = null;

        if (
            pagerElDarkroom &&
            isDarkroom &&
            lastRestoredDarkroom !== pagerElDarkroom
        ) {
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

    // Copy, don't alias: $state() proxies the object it is given, so passing the
    // exported default directly would write every slider drag back into it.
    let sliders: Sliders = $state({ ...defaultSlidersRGB });

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
            const colorKeys = Object.keys(overridesBW) as (keyof Sliders)[];
            for (const k of colorKeys) {
                (savedColorValues as any)[k] = sliders[k];
            }
            animateObject(
                sliders as unknown as Record<string, number>,
                overridesBW as Record<string, number>,
            );
            if (activePageNormal > 2) {
                await tick(); // wait for the {#if isRgb} block to unmount
                scrollToPage(2);
            }
        } else if (savedColorValues) {
            animateObject(
                sliders as unknown as Record<string, number>,
                savedColorValues as Record<string, number>,
            );
            savedColorValues = null;
        }
    }

    // ===== Invert toggle logic =====

    function handleInvertToggle(isInverted: boolean) {
        commit({
            type: "slider",
            key: "invert",
            oldValue: !isInverted,
            newValue: isInverted,
        });
    }

    // ===== Image =====

    let imagePayload: ImagePayload | null = $state(null);

    async function handleOpenImage() {
        imagePayload = await openImage();
    }

    let isExportPending = $state(false);
    let showExportModal = $state(false);
    let exportMenuOpen = $state(false);
    let exportSettings = $state<ExportSettings>({ ...defaultExportSettings });
    let footerEl: HTMLDivElement | null = $state(null);

    $effect(() => {
        if (!exportMenuOpen) return;
        function handleDocClick(e: MouseEvent) {
            if (!footerEl?.contains(e.target as Node)) {
                exportMenuOpen = false;
            }
        }
        document.addEventListener("click", handleDocClick);
        return () => document.removeEventListener("click", handleDocClick);
    });

    async function handleExport() {
        isExportPending = true;
        const unlisten = await listen("export:started", () => {
            showExportModal = true;
        });
        try {
            await exportImage(sliders, exportSettings);
        } finally {
            unlisten();
            isExportPending = false;
            showExportModal = false;
        }
    }

    // ===== History =====
    const stateAccessors: StateAccessors = {
        getSlider: (key) => sliders[key as keyof Sliders],
        setSlider: (key, val) => {
            (sliders as unknown as Record<string, typeof val>)[key] = val;
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

    // Frame stats start visible in a dev run and hidden in a release build, so
    // the numbers are there while working without shipping to a user. F1 flips
    // it either way — a release build still needs to be measurable.
    frameStats.enabled = import.meta.env.DEV;

    const handleKeydown = createKeydownHandler({
        undo: undo,
        redo: redo,
        movePage: movePage,
        changeMode: handleModeToggle,
        openImage: handleOpenImage,
        toggleFrameStats: () => {
            frameStats.enabled = !frameStats.enabled;
            if (!frameStats.enabled) frameStats.clear();
        },
    });
</script>

<svelte:window onkeydown={handleKeydown} />

<TitleBar {undo} {redo} open={handleOpenImage}></TitleBar>
<!-- Full-window canvas background. The image is centred on .canvas-slot but is
     free to spill out of it in every direction once zoomed; the toolbar and
     side bar sit on a higher layer and simply paint over the overflow. -->
<div class="canvas-backdrop"></div>
<div class="app-shell flex absolute flex-row gap-3">
    <div class="toolbar glass shrink-0 w-8 rounded-[8px]"></div>
    <div class="canvas-slot relative flex-1">
        <div class="canvas-center absolute flex items-center justify-center">
            <PreviewImageCanvas {sliders} {imagePayload} />
        </div>
        {#if frameStats.enabled}
            <FrameStatsOverlay />
        {/if}
    </div>
    <div class="side-bar flex w-[23%] flex-col gap-3">
        <div class="histogram-panel glass shrink-0 rounded-[12px]">
            <div class="histogram-container m-1.25">
                <Histogram {sliders} {imagePayload} />
            </div>
        </div>
        <div class="tools-panel glass flex rounded-[12px] flex-1 flex-col">
            <div
                class="quick-actions flex items-center flex-row gap-2 ml-3 mt-2.5"
                class:dimmed={exportMenuOpen}
            >
                <ColorModeToggle bind:isRgb onToggle={handleColorModeToggle} />
                <!-- Stays mounted and fades. It is the last item in a
                     left-aligned row, so holding its width costs no layout —
                     and a plain opacity crossfade never touches layout at all,
                     where `slide` animated width/padding/margin and reflowed
                     the row on every frame. -->
                <div class="invert-slot" class:shown={isDarkroom}>
                    <InvertToggle bind:isInverted={sliders.invert} onToggle={handleInvertToggle} />
                </div>
            </div>
            <div class="controls-section flex relative flex-1 flex-col" class:dimmed={exportMenuOpen}>
                <div
                    class="controls-pager-wrapper relative flex-1"
                    class:sliding={isModeSliding}
                >
                    {#if !isDarkroom}
                        <div
                            class="controls-pager absolute flex z-1"
                            bind:this={pagerElNormal}
                            onscroll={handleNormalPagerScroll}
                            onwheel={handlePagerWheel}
                            onwheelcapture={handlePagerWheel}
                            transition:slideX={{ direction: -1 }}
                        >
                            <!-- Sharpness Panel -->
                            <div class="controls-page">
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <span class="section-title pb-1.5">Sharpness</span>
                                    <SingleValSliderGroup
                                        bind:value={sliders.clarity}
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
                                            commit({
                                                type: "slider",
                                                key: "clarity",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.texture}
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
                                            commit({
                                                type: "slider",
                                                key: "texture",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                </div>
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <span class="section-title pb-1.5"
                                        >Unsharp Mask</span
                                    >
                                    <div
                                        class="flex w-full aspect-[1.3/1] rounded-[6px]"
                                        style="background: black;"
                                    ></div>
                                    <div class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"></div>
                                    <SingleValSliderGroup
                                        bind:value={sliders.usmAmount}
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
                                            commit({
                                                type: "slider",
                                                key: "usmAmount",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.usmRadius}
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
                                            commit({
                                                type: "slider",
                                                key: "usmRadius",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.usmLumaThreshold}
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
                                            commit({
                                                type: "slider",
                                                key: "usmLumaThreshold",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.usmDetailThreshold}
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
                                            commit({
                                                type: "slider",
                                                key: "usmDetailThreshold",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                </div>
                                <div class="flex h-7.5"></div>
                            </div>
                            <!-- Light & Color Panel -->
                            <div class="controls-page">
                                {#if isRgb}
                                    <div
                                        class="slider-section flex flex-col mt-4 ml-4 mr-3"
                                        transition:slide={{ duration: 200 }}
                                    >
                                        <span class="section-title pb-1.5"
                                            >White Balance</span
                                        >
                                        <SingleValSliderGroup
                                            bind:value={sliders.wbTemp}
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
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <span class="section-title pb-1.5">
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
                                        min={-100}
                                        max={100}
                                        decimalPlaces={1}
                                        dragStep={0.1}
                                        sliderStep={1}
                                        keyboardStep={0.1}
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
                                                class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"
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
                                    <div class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"></div>
                                    <SingleValSliderGroup
                                        bind:value={sliders.highlights}
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
                                            commit({
                                                type: "slider",
                                                key: "highlights",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.shadows}
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
                                            commit({
                                                type: "slider",
                                                key: "shadows",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.whites}
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
                                            commit({
                                                type: "slider",
                                                key: "whites",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.blacks}
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
                                            commit({
                                                type: "slider",
                                                key: "blacks",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                </div>
                                <div class="flex h-7.5"></div>
                            </div>
                            <!-- Curves Panel -->
                            <div class="controls-page">
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <span class="section-title pb-1.5">Curves</span>
                                    <div
                                        class="flex w-full aspect-square rounded-[6px]"
                                        style="background: black;"
                                    ></div>
                                    <div class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"></div>
                                    <SingleValSliderGroup
                                        bind:value={sliders.highlights}
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
                                            commit({
                                                type: "slider",
                                                key: "highlights",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.shadows}
                                        defaultValue={0}
                                        name="Shadows"
                                        min={-100}
                                        max={100}
                                        decimalPlaces={1}
                                        dragStep={0.1}
                                        sliderStep={1}
                                        keyboardStep={0.1}
                                        gradientStartColor={"#303030"}
                                        gradientEndColor={"#3f3f3f"}
                                        onCommit={(oldVal, newVal) =>
                                            commit({
                                                type: "slider",
                                                key: "shadows",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.whites}
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
                                            commit({
                                                type: "slider",
                                                key: "whites",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                    <SingleValSliderGroup
                                        bind:value={sliders.blacks}
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
                                            commit({
                                                type: "slider",
                                                key: "blacks",
                                                oldValue: oldVal,
                                                newValue: newVal,
                                            })}
                                    ></SingleValSliderGroup>
                                </div>
                                <div class="flex h-7.5"></div>
                            </div>
                            <!-- HSL Panel -->
                            {#if isRgb}
                                <div class="controls-page">
                                    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                        <span class="section-title pb-1.5">HSL</span>
                                        <SingleValSliderGroup
                                            bind:value={sliders.hue}
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
                                                commit({
                                                    type: "slider",
                                                    key: "hue",
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
                                        <div class="sliders-separator h-0.5 rounded-[1px] mt-3 mb-2 mr-1.5"></div>
                                    </div>
                                </div>
                                <div class="flex h-7.5"></div>
                            {/if}
                        </div>
                    {/if}
                    {#if isDarkroom}
                        <div
                            class="controls-pager absolute flex z-1"
                            bind:this={pagerElDarkroom}
                            onscroll={handleDarkroomPagerScroll}
                            onwheel={handlePagerWheel}
                            onwheelcapture={handlePagerWheel}
                            transition:slideX={{ direction: 1 }}
                        >
                            <div class="controls-page">
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <TripleValSliderHistGroup
                                        name="Red Input"
                                        bind:valueA={sliders.redBlackPoint}
                                        bind:valueB={sliders.redGamma}
                                        bind:valueC={sliders.redWhitePoint}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={imagePayload?.histR ?? null}
                                        histogramChannel="r"
                                    ></TripleValSliderHistGroup>
                                    <TripleValSliderHistGroup
                                        name="Green Input"
                                        bind:valueA={sliders.greenBlackPoint}
                                        bind:valueB={sliders.greenGamma}
                                        bind:valueC={sliders.greenWhitePoint}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={imagePayload?.histG ?? null}
                                        histogramChannel="g"
                                    ></TripleValSliderHistGroup>
                                    <TripleValSliderHistGroup
                                        name="Blue Input"
                                        bind:valueA={sliders.blueBlackPoint}
                                        bind:valueB={sliders.blueGamma}
                                        bind:valueC={sliders.blueWhitePoint}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={imagePayload?.histB ?? null}
                                        histogramChannel="b"
                                    ></TripleValSliderHistGroup>
                                    <DoubleValSliderGroup
                                        name="RGB Output"
                                        bind:valueA={sliders.rgbOutputBlack}
                                        bind:valueB={sliders.rgbOutputWhite}
                                        defaultValueA={0}
                                        defaultValueB={255}
                                        min={0}
                                        max={255}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                    ></DoubleValSliderGroup>
                                </div>
                                <div class="flex h-7.5"></div>
                            </div>
                        </div>
                    {/if}
                </div>
            </div>
        </div>
        <div class="side-panel-footer glass flex relative justify-center items-center h-12 rounded-[12px] shrink-0" bind:this={footerEl}>
            {#if exportMenuOpen}
                <ExportSettingsMenu bind:settings={exportSettings} {isRgb} />
            {/if}
            <div class="absolute" style="left: 10px;">
                <ExportButton onexport={handleExport} bind:menuOpen={exportMenuOpen} settings={exportSettings}></ExportButton>
            </div>
            <div class="page-dots-wrapper relative flex justify-center items-center">
                    <!-- Both groups stay mounted and crossfade on opacity
                         alone. `blur` animated filter: blur(), and these sit
                         inside .side-panel-footer — a backdrop-filter surface,
                         which had to re-run its own blur on every frame the
                         dots were damaging it. Staying mounted also retires
                         the outgoing/z-index bookkeeping the outro needed. -->
                    <div
                        class="page-dots absolute z-1 flex justify-center items-center gap-3"
                        class:shown={!isDarkroom}
                        aria-hidden={isDarkroom}
                    >
                        {#each Array(isRgb ? 4 : 3) as _, i}
                            <button
                                class="dot size-1.5 rounded-full border-0 cursor-pointer p-0"
                                class:active={activePageNormal === i}
                                onclick={() => scrollToPage(i)}
                                aria-label="Go to page {i + 1}"
                                aria-current={activePageNormal === i
                                    ? "step"
                                    : undefined}
                            ></button>
                        {/each}
                    </div>
                    <div
                        class="page-dots absolute z-1 flex justify-center items-center gap-3"
                        class:shown={isDarkroom}
                        aria-hidden={!isDarkroom}
                    >
                        <button
                            class="dot active size-1.5 rounded-full border-0 cursor-pointer p-0"
                            aria-label="Darkroom page"
                            aria-current="step"
                        ></button>
                    </div>
            </div>
            <div class="absolute" style="right: 10px;">
                <ModeToggle bind:isDarkroom onModeToggle={handleModeToggle}
                ></ModeToggle>
            </div>
        </div>
    </div>
</div>

<ExportModal visible={showExportModal}></ExportModal>

<style>
    .canvas-backdrop {
        position: fixed;
        inset: 0;
        background: var(--canvasBackdrop);
        transition: background-color 0.12s ease;
        z-index: 0;
    }

    .app-shell {
        /* 32px title bar + the same 12px inset used on the other three sides */
        top: 44px;
        bottom: 12px;
        left: 12px;
        right: 12px;
        z-index: 1;
        /* The shell only reserves space; clicks fall through to whatever layer
           each child opts back into. */
        pointer-events: none;
    }

    /* The flex row still defines where the image is centred, but the slot never
       clips: overflow spills across the whole window and is occluded by the
       panels, which are painted later and lifted above it. */
    .canvas-slot {
        min-width: 0;
        min-height: 0;
        overflow: visible;
    }

    .canvas-center {
        inset: 36px; /* breathing room so a fitted image never touches the panels */
        overflow: visible;
        pointer-events: auto;
    }

    /* ===== Glass panels =====
       The material itself lives in styles/glass.css, shared with the title
       bar's chips; each panel opts in with `class="glass"` in the markup. Only
       the drop shadow and the lighting angle change with panel size, so those
       are all that is left here. */

    /* small panels — the shadow colour is near-black rather than #1f1f1f, which
       was only a hair darker than the #262626 backdrop and so read as nothing */
    .histogram-panel,
    .side-panel-footer {
        box-shadow:
            6px 10px 22px -2px rgba(0, 0, 0, 0.47),
            2px 3px 8px -1px rgba(0, 0, 0, 0.34),
            inset 16px 21px 50px -38px rgba(255, 255, 255, 0.16);
    }

    /* the export settings menu opens upward, out of the footer's own box */
    .side-panel-footer {
        overflow: visible;
    }

    /* large panel — only the one holding the sliders. Its inset highlight is
       pulled right back: at full strength the smear across the top-left of a
       680px-tall panel fought with the sliders sitting on top of it. */
    .tools-panel {
        --panel-angle: 146deg;
        box-shadow:
            10px 16px 38px -4px rgba(0, 0, 0, 0.51),
            3px 5px 12px -2px rgba(0, 0, 0, 0.36),
            inset 20px 30px 60px -42px rgba(255, 255, 255, 0.17);
    }

    /* The toolbar is a 42px strip, so the same rim reads twice as bright per
       unit of width as it does on the wide panels. Its shadow is mirrored to
       cast left, away from the canvas, and sits lighter than the right-hand
       panels' since there is far less panel to lift.

       Mirroring the shadow puts this panel's light source in the upper right,
       where every other panel's is in the upper left, so both highlights have
       to mirror with it or the strip ends up lit from two directions at once:
       the rim gradient flips about the vertical axis (360 - 142) and the inset
       smear moves to the top-right corner. */
    .toolbar {
        --panel-angle: 218deg;
        --rim-hi: 0.13;
        --rim-mid: 0.07;
        --rim-lo: 0.05;
        --rim-end: 0.08;
        box-shadow:
            -6px 10px 22px -2px rgba(0, 0, 0, 0.33),
            -2px 3px 8px -1px rgba(0, 0, 0, 0.24),
            inset -16px 21px 50px -38px rgba(255, 255, 255, 0.16);
        min-width: 42px;
        z-index: 10;
        pointer-events: auto;
    }

    .side-bar {
        min-width: 300px;
        position: relative;
        z-index: 10;
        pointer-events: auto;
    }

    .quick-actions {
        overflow: hidden;
        transition: opacity 0.2s ease;
    }

    .tools-panel {
        min-height: 0; /* allows it to shrink below content size */
    }
    .tools-panel::-webkit-scrollbar {
        display: none;
    } /* Chrome/Safari/Edge */

    .controls-section {
        min-height: 0;
        overflow: hidden;
        transition: opacity 0.2s ease;
    }

    .dimmed {
        opacity: 0.25;
        pointer-events: none;
    }

    .controls-pager-wrapper {
        min-height: 0;
        overflow: hidden;
    }

    /* Held only for the length of the slide (see MODE_SLIDE_MS). Both masks go
       at once: each is an offscreen surface, and they are nested, so leaving
       either in place keeps the compositing chain the suspension exists to
       break. The wrapper's own `overflow: hidden` still clips the strip, so
       what is lost is the softness of the cut, not the cut itself. */
    .controls-pager-wrapper.sliding .controls-pager,
    .controls-pager-wrapper.sliding .controls-page {
        mask-image: none;
        -webkit-mask-image: none;
    }

    .controls-pager {
        inset: 0;
        overflow-x: auto;
        overflow-y: hidden;
        scroll-snap-type: x mandatory;
        scrollbar-width: none;
        backface-visibility: hidden;

        /* Side fades, mirroring the top/bottom ones on .controls-page. The mask
           sits on the scroll frame rather than the content, so it stays pinned
           to the panel's edges while the pages travel underneath it.

           The ramps stop where the page's own side margins do — 16px (ml-4) and
           12px (mr-3) — so a snapped page is untouched and only content in
           transit passes through the gradient. */
        mask-image: linear-gradient(
            to right,
            transparent 0,
            black 16px,
            black calc(100% - 12px),
            transparent 100%
        );
        -webkit-mask-image: linear-gradient(
            to right,
            transparent 0,
            black 16px,
            black calc(100% - 12px),
            transparent 100%
        );
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

    .section-title {
        font-family: "Figtree", sans-serif;
        font-size: 18px;
        font-weight: 600;
        color: var(--color1);
    }

    .sliders-separator {
        background: var(--color5);
    }

    .page-dots-wrapper {
        min-width: 72px; /* fit up to 4 dots + gaps */
    }

    /* Both groups are always mounted, stacked on the same centre, and only
       opacity separates them. `visibility` steps at the far end of the fade so
       the hidden group stops taking clicks and leaves the tab order, without
       being display:none — which would kill the transition outright. */
    .page-dots-wrapper .page-dots {
        left: 50%;
        top: 50%;
        transform: translate(-50%, -50%);
        backface-visibility: hidden;
        opacity: 0;
        visibility: hidden;
        transition:
            opacity 0.5s ease,
            visibility 0s linear 0.5s;
    }
    .page-dots-wrapper .page-dots.shown {
        opacity: 1;
        visibility: visible;
        transition:
            opacity 0.5s ease,
            visibility 0s linear 0s;
    }

    /* Same crossfade, quicker, matching the 250ms the slide used to take. */
    .invert-slot {
        opacity: 0;
        visibility: hidden;
        transition:
            opacity 0.25s ease,
            visibility 0s linear 0.25s;
    }
    .invert-slot.shown {
        opacity: 1;
        visibility: visible;
        transition:
            opacity 0.25s ease,
            visibility 0s linear 0s;
    }

    .dot {
        background: var(--color5);
        transition:
            background 0.2s,
            transform 0.2s;
    }
    .dot.active {
        background: var(--color1);
        transform: scale(1.3);
    }
</style>
