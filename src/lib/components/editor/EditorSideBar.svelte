<!-- lib/components/editor/EditorSideBar.svelte -->
<script lang="ts">
    import { cubicInOut } from "svelte/easing";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import ColorModeToggle from "$lib/components/inputs/colorModeToggle.svelte";
    import InvertToggle from "$lib/components/inputs/invertToggle.svelte";
    import AutoLevelsButton from "$lib/components/inputs/autoLevelsButton.svelte";
    import RevertButton from "$lib/components/inputs/revertButton.svelte";
    import ExportButton from "$lib/components/inputs/exportButton.svelte";
    import { exportDialog } from "$lib/export/exportDialog.svelte";
    import ModeToggle from "$lib/components/inputs/modeToggle.svelte";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
    import { loupe } from "$lib/gpu/loupe.svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { editor, MODE_SLIDE_MS } from "$lib/editor/editor.svelte";
    import LighterRoomPages from "./LighterRoomPages.svelte";
    import DarkroomPages from "./DarkroomPages.svelte";

    // The pagers sit on one strip, lighter room left and darkroom right, and each
    // slides in and out on its own side. The travel is a percentage of the
    // pager's width, which `fly` cannot express.
    function slideX(
        node: Element,
        { direction = 1, duration = MODE_SLIDE_MS }: { direction?: number; duration?: number } = {},
    ) {
        return {
            duration,
            easing: cubicInOut,
            css: (_t: number, u: number) =>
                `transform: translateX(${direction * u * 100}%);`,
        };
    }

    function handleNormalPagerScroll() {
        const el = editor.pagerElNormal;
        if (!el) return;
        editor.activePageNormal = Math.round(el.scrollLeft / el.clientWidth);
    }

    function handleDarkroomPagerScroll() {
        const el = editor.pagerElDarkroom;
        if (!el) return;
        editor.activePageDarkroom = Math.round(el.scrollLeft / el.clientWidth);
    }

    function handlePagerWheel(e: WheelEvent) {
        // only intercept vertical intent
        if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return;

        const pages = editor.pagerEl?.querySelectorAll<HTMLElement>(".controls-page");
        const page = pages?.[editor.activePage];
        if (!page) return;

        // Only if that page can actually scroll vertically
        if (page.scrollHeight <= page.clientHeight) return;

        e.preventDefault();
        page.scrollBy({ top: e.deltaY, behavior: "auto" });
    }

    /* Mouse side buttons page the strip. Without the preventDefault they walk
       the webview's history instead. */
    function handlePagerSideButton(e: MouseEvent) {
        if (e.button !== 3 && e.button !== 4) return;
        e.preventDefault();
        if (e.type === "pointerdown") editor.movePage(e.button === 4);
    }

    // A pager mounting again picks up the page it was left on.
    let lastRestoredNormal: HTMLDivElement | null = null;
    let lastRestoredDarkroom: HTMLDivElement | null = null;
    $effect(() => {
        const { pagerElNormal, pagerElDarkroom, isDarkroom } = editor;
        if (pagerElNormal && !isDarkroom && lastRestoredNormal !== pagerElNormal) {
            lastRestoredNormal = pagerElNormal;
            pagerElNormal.scrollLeft = editor.activePageNormal * pagerElNormal.clientWidth;
        }
        if (isDarkroom) lastRestoredNormal = null;

        if (pagerElDarkroom && isDarkroom && lastRestoredDarkroom !== pagerElDarkroom) {
            lastRestoredDarkroom = pagerElDarkroom;
            pagerElDarkroom.scrollLeft = editor.activePageDarkroom * pagerElDarkroom.clientWidth;
        }
        if (!isDarkroom) lastRestoredDarkroom = null;
    });

    // Any press in the sidebar but on the loupe itself settles where it is.
    function endLoupeOnPress(e: PointerEvent) {
        if (!loupe.picking) return;
        if (e.target instanceof Element && e.target.closest("[data-loupe]")) return;
        loupe.picking = false;
    }
</script>

<!-- Holds the side bar's room in the row; the bar itself only slides. -->
<div class="side-bar-space shrink-0" class:away={editor.isRetouching}></div>
<div
    class="side-bar flex flex-col gap-3"
    class:away={editor.isRetouching}
    inert={editor.isRetouching}
    onpointerdowncapture={endLoupeOnPress}
>
    <div
        class="histogram-panel glass shrink-0 rounded-[12px]"
        class:dimmed={settings.debugTools && !debugStats.histogramEnabled}
    >
        <div
            class="histogram-container m-1.25"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
            <Histogram
                sliders={editor.shownSliders}
                curveLut={editor.shownCurveLut}
                imagePayload={editor.imagePayload}
                crop={editor.edit.crop}
                retouch={editor.histogramStrokes}
                curveCanvas={editor.curveHistogramCanvas}
                masterCanvas={editor.masterHistogramCanvas}
                curveChannel={editor.edit.isRgb ? editor.curveChannel : "master"}
            />
        </div>
    </div>
    <div class="tools-panel glass flex rounded-[12px] flex-1 flex-col">
        <div
            class="quick-actions flex items-center flex-row gap-2 ml-3 mr-3 mt-2.5"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
            <ColorModeToggle bind:isRgb={editor.edit.isRgb} onToggle={editor.toggleColorMode} />
            <!-- Stays mounted and fades on opacity alone, so the row never
                 reflows. -->
            <div class="invert-slot flex items-center gap-2" class:shown={editor.isDarkroom}>
                <InvertToggle bind:isInverted={editor.edit.sliders.invert} onToggle={editor.toggleInvert} />
                <AutoLevelsButton disabled={!editor.inputHistograms} onauto={editor.autoLevels} />
            </div>
            <!-- Pushed to the far end of the row, opposite the toggles. -->
            <div class="ml-auto">
                <RevertButton
                    visible={!editor.isIdentity}
                    onrevert={editor.resetAdjustments}
                />
            </div>
        </div>
        <div
            class="controls-section flex relative flex-1 flex-col"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
            <div
                class="controls-pager-wrapper relative flex-1"
                class:sliding={editor.isModeSliding}
            >
                {#if !editor.isDarkroom}
                    <div
                        class="controls-pager absolute flex z-1"
                        bind:this={editor.pagerElNormal}
                        onscroll={handleNormalPagerScroll}
                        onwheel={handlePagerWheel}
                        onwheelcapture={handlePagerWheel}
                        onpointerdown={handlePagerSideButton}
                        onauxclick={handlePagerSideButton}
                        role="group"
                        transition:slideX={{ direction: -1 }}
                    >
                        <LighterRoomPages />
                    </div>
                {/if}
                {#if editor.isDarkroom}
                    <div
                        class="controls-pager absolute flex z-1"
                        bind:this={editor.pagerElDarkroom}
                        onscroll={handleDarkroomPagerScroll}
                        onwheel={handlePagerWheel}
                        onwheelcapture={handlePagerWheel}
                        onpointerdown={handlePagerSideButton}
                        onauxclick={handlePagerSideButton}
                        role="group"
                        transition:slideX={{ direction: 1 }}
                    >
                        <DarkroomPages />
                    </div>
                {/if}
            </div>
        </div>
    </div>
    <div class="side-panel-footer glass flex relative justify-center items-center h-12 rounded-[12px] shrink-0">
        <div
            class="footer-item absolute"
            style="left: 10px;"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
            <ExportButton onclick={exportDialog.show} />
        </div>
        <div
            class="footer-item page-dots-wrapper relative flex justify-center items-center"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
                <!-- Both groups stay mounted and crossfade on opacity alone,
                     which the footer's backdrop blur can composite cheaply. -->
                <div
                    class="page-dots absolute z-1 flex justify-center items-center gap-3"
                    class:shown={!editor.isDarkroom}
                    aria-hidden={editor.isDarkroom}
                >
                    {#each Array(3) as _, i}
                        <button
                            class="dot size-1.5 rounded-full border-0 cursor-pointer p-0"
                            class:active={editor.activePageNormal === i}
                            onclick={() => editor.scrollToPage(i)}
                            aria-label="Go to page {i + 1}"
                            aria-current={editor.activePageNormal === i
                                ? "step"
                                : undefined}
                        ></button>
                    {/each}
                </div>
                <div
                    class="page-dots absolute z-1 flex justify-center items-center gap-3"
                    class:shown={editor.isDarkroom}
                    aria-hidden={!editor.isDarkroom}
                >
                    {#each Array(2) as _, i}
                        <button
                            class="dot size-1.5 rounded-full border-0 cursor-pointer p-0"
                            class:active={editor.activePageDarkroom === i}
                            onclick={() => editor.scrollToPage(i)}
                            aria-label="Go to darkroom page {i + 1}"
                            aria-current={editor.activePageDarkroom === i
                                ? "step"
                                : undefined}
                        ></button>
                    {/each}
                </div>
        </div>
        <div
            class="footer-item absolute"
            style="right: 10px;"
            class:dimmed={editor.sidebarInactive}
            inert={editor.sidebarInactive}
        >
            <ModeToggle bind:isDarkroom={editor.isDarkroom} onModeToggle={editor.toggleMode}
            ></ModeToggle>
        </div>
    </div>
</div>

<style>
    /* small panels — the shadow colour is near-black rather than #1f1f1f, which
       was only a hair darker than the #262626 backdrop and so read as nothing */
    .histogram-panel,
    .side-panel-footer {
        box-shadow:
            6px 10px 22px -2px rgba(0, 0, 0, 0.47),
            2px 3px 8px -1px rgba(0, 0, 0, 0.34),
            inset 16px 21px 50px -38px rgba(255, 255, 255, 0.16);
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

    .footer-item {
        transition: opacity 0.2s ease;
    }

    /* While a brush is in hand it gives its room and its gap to the image.
       Back only once the brush bar has dropped away. */
    .side-bar-space {
        width: max(23%, 300px);
        transition:
            width 0.35s cubic-bezier(0.2, 0, 0, 1) 0.25s,
            margin-left 0.35s cubic-bezier(0.2, 0, 0, 1) 0.25s;
    }

    .side-bar-space.away {
        width: 0;
        margin-left: -12px;
        transition-delay: 0s;
    }

    /* Over its space rather than in the row, so leaving moves it by transform
       alone and never lays it out or repaints it. */
    .side-bar {
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        width: max(23%, 300px);
        /* A faint lift so labels and values hold up over a bright image. */
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
        z-index: 10;
        pointer-events: auto;
        transition: transform 0.35s cubic-bezier(0.2, 0, 0, 1) 0.25s;
    }

    /* Past the window's right edge, far enough that the panels' shadows go too. */
    .side-bar.away {
        transform: translateX(calc(100% + 52px));
        transition-delay: 0s;
    }

    .quick-actions {
        overflow: hidden;
        transition: opacity 0.2s ease;
    }

    .histogram-container {
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

    .controls-pager-wrapper {
        min-height: 0;
        overflow: hidden;
    }

    /* Both nested masks are dropped for the length of the slide, so the strip
       moves as a plain transform. The wrapper still clips it. */
    .controls-pager-wrapper.sliding .controls-pager,
    .controls-pager-wrapper.sliding :global(.controls-page) {
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

        /* Side fades pinned to the scroll frame. The ramps end at the page's
           own margins (ml-4, mr-3), so a snapped page is untouched. */
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

    .page-dots-wrapper {
        min-width: 72px; /* fit up to 4 dots + gaps */
    }

    /* Both groups are stacked on the same centre. `visibility` steps at the end
       of the fade, so the hidden group leaves the tab order and takes no clicks. */
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
