<!-- routes/+page.svelte -->
<script lang="ts">
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { slide } from "svelte/transition";
    import { cubicInOut } from "svelte/easing";

    import TitleBar from "$lib/components/window/TitleBar.svelte";

    import Histogram from "$lib/components/outputs/Histogram.svelte";
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import CropOverlay from "$lib/components/outputs/CropOverlay.svelte";
    import Loupe from "$lib/components/outputs/Loupe.svelte";
    import { loupe } from "$lib/gpu/loupe.svelte";
    import DebugStatsOverlay from "$lib/components/outputs/DebugStatsOverlay.svelte";
    import ColorSpacePanel from "$lib/components/outputs/ColorSpacePanel.svelte";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import ColorModeToggle from "$lib/components/inputs/colorModeToggle.svelte";
    import InvertToggle from "$lib/components/inputs/invertToggle.svelte";
    import AutoLevelsButton from "$lib/components/inputs/autoLevelsButton.svelte";
    import RevertButton from "$lib/components/inputs/revertButton.svelte";
    import CropButton from "$lib/components/inputs/cropButton.svelte";
    import ZoomButton from "$lib/components/inputs/zoomButton.svelte";
    import CropTools from "$lib/components/inputs/cropTools.svelte";
    import SingleValSliderGroup from "$lib/components/inputs/values/SingleValSliderGroup.svelte";
    import TripleValSliderHistGroup from "$lib/components/inputs/values/TripleValSliderHistGroup.svelte";
    import DoubleValSliderGroup from "$lib/components/inputs/values/DoubleValSliderGroup.svelte";
    import HslBandGroup from "$lib/components/inputs/values/HslBandGroup.svelte";
    import CurveEditor from "$lib/components/inputs/CurveEditor.svelte";
    import ExportButton from "$lib/components/inputs/exportButton.svelte";
    import ExportSettingsMenu from "$lib/components/inputs/ExportSettingsMenu.svelte";
    import ModeToggle from "$lib/components/inputs/modeToggle.svelte";
    import ExportModal from "$lib/components/ExportModal.svelte";
    import CustomAspectModal from "$lib/components/CustomAspectModal.svelte";
    import PresetsButton from "$lib/components/inputs/presetsButton.svelte";
    import PresetsPanel from "$lib/components/presets/PresetsPanel.svelte";
    import PresetModal from "$lib/components/presets/PresetModal.svelte";
    import { presets } from "$lib/presets/presets.svelte";
    import {
        type Preset,
        captureFields,
        changedFieldIds,
        modeFieldIds,
        presetFieldIds,
        revisePreset,
    } from "$lib/presets/presetDocument";

    import {
        type Sliders,
        defaultSlidersRGB,
        defaultSlidersBW,
        overridesBW,
        modeKeys,
        HSL_KEYS,
        SLIDER_TOLERANCE,
    } from "$lib/types/imgParameters";

    import {
        type CurveChannel,
        type Curves,
        bakeCurveLut,
        cloneCurves,
        curvesEqual,
        defaultCurves,
    } from "$lib/types/curves";

    import { type ImagePayload } from "$lib/types/imagePayload";
    import {
        type Crop,
        FULL_FRAME,
        cropsEqual,
        isFullFrame,
        turnCrop,
        flipCrop,
    } from "$lib/types/crop";
    import {
        type AspectLock,
        FREE_ASPECT,
        aspectRatio,
        aspectsMatch,
        cropAspect,
        cropWithAspect,
    } from "$lib/types/cropAspect";
    import { cropHistograms, type ChannelHistograms } from "$lib/utils/cropHistograms";
    import { autoLevels } from "$lib/utils/autoLevels";
    import { openImage } from "$lib/utils/openImage";
    import { holdPageZoom } from "$lib/utils/pageZoomGuard";
    import { copyAdjustments, readAdjustments } from "$lib/utils/adjustmentsClipboard";
    import { exportImage } from "$lib/utils/exportImage";
    import { type ExportSettings, defaultExportSettings } from "$lib/types/exportSettings";
    import { proofing } from "$lib/gpu/proofing.svelte";

    import { animateObject } from "$lib/utils/animateObject";

    import { history } from "$lib/history/history.svelte";
    import { applyAction, undoAction, type StateAccessors, } from "$lib/history/historyDispatch";
    import type { Action } from "$lib/types/historyActions";
    import { actionMode, actionPage } from "$lib/history/actionControls";

    import { createKeydownHandler, isTextField } from "$lib/config/keyboardShortcuts";

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

    $effect(() => holdPageZoom());

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
    let activePageDarkroom = $state(1);
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

    /* Mouse side buttons page the strip. Without the preventDefault they walk
       the webview's history instead. */
    function handlePagerSideButton(e: MouseEvent) {
        if (e.button !== 3 && e.button !== 4) return;
        e.preventDefault();
        if (e.type === "pointerdown") movePage(e.button === 4);
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
            pagerElDarkroom.scrollLeft =
                activePageDarkroom * pagerElDarkroom.clientWidth;
        }
        if (!isDarkroom) lastRestoredDarkroom = null;
    });

    // Jumps without scrolling. A pager that is not mounted yet picks the page
    // up from its active page when it is.
    function jumpToPage(darkroom: boolean, index: number) {
        if (darkroom) activePageDarkroom = index;
        else activePageNormal = index;
        const el = darkroom ? pagerElDarkroom : pagerElNormal;
        if (el && darkroom === isDarkroom) {
            el.scrollTo({ left: index * el.clientWidth, behavior: "instant" });
        }
    }

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

    let curves: Curves = $state(defaultCurves());
    let curveHistogramCanvas: HTMLCanvasElement | null = $state(null);
    let masterHistogramCanvas: HTMLCanvasElement | null = $state(null);
    let curveChannel: CurveChannel = $state("master");

    // ===== B&W toggle logic =====
    let isRgb = $state(true);
    let savedColorValues: Partial<typeof sliders> | null = null;

    /* The colour keys are in transit for as long as the toggle animates them,
       so anything reading them has to wait rather than judge them mid-flight —
       see isIdentity, which would otherwise flicker on the way back to colour.
       Passed to animateObject so the two cannot drift apart. */
    const COLOR_ANIM_MS = 200;
    let isColorAnimating = $state(false);
    let colorAnimTimer: ReturnType<typeof setTimeout> | null = null;

    function markColorAnimating() {
        isColorAnimating = true;
        if (colorAnimTimer) clearTimeout(colorAnimTimer);
        colorAnimTimer = setTimeout(() => {
            isColorAnimating = false;
            colorAnimTimer = null;
        }, COLOR_ANIM_MS);
    }

    let activePage = $derived(
        isDarkroom ? activePageDarkroom : activePageNormal,
    );
    let pageCount = $derived(isDarkroom ? 2 : 3);

    // B&W leaves the colour curves out, the way it pins the colour sliders.
    let curveLut = $derived(bakeCurveLut(curves, isRgb));
    let pagerEl = $derived<HTMLDivElement | null>(
        isDarkroom ? pagerElDarkroom : pagerElNormal,
    );

    function handleColorModeToggle(isBw: boolean) {
        if (isBw) {
            savedColorValues = {};
            const colorKeys = Object.keys(overridesBW) as (keyof Sliders)[];
            for (const k of colorKeys) {
                (savedColorValues as any)[k] = sliders[k];
            }
            animateObject(
                sliders as unknown as Record<string, number>,
                overridesBW as Record<string, number>,
                COLOR_ANIM_MS,
            );
            markColorAnimating();
        } else if (savedColorValues) {
            animateObject(
                sliders as unknown as Record<string, number>,
                savedColorValues as Record<string, number>,
                COLOR_ANIM_MS,
            );
            markColorAnimating();
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

    // A failed open has already cost the backend the image it was holding, so
    // the view is cleared to match rather than left showing pixels that can no
    // longer be exported. A dismissed picker returns null and changes nothing.
    async function handleOpenImage() {
        try {
            const payload = await openImage();
            if (payload) {
                imagePayload = payload;
                zoom = null;
                // A crop describes one frame, so neither it nor its history
                // carries over the way the adjustments do.
                crop = { ...FULL_FRAME };
                cancelCrop();
                history.discard((a) => a.type === "crop");
            }
        } catch (e) {
            imagePayload = null;
            console.error("Failed to open image:", e);
        }
    }

    // ===== Crop =====

    // What is applied, and what the overlay is editing while the tool is open.
    // The preview shows the whole frame for as long as the tool is.
    let crop: Crop = $state({ ...FULL_FRAME });
    let draftCrop: Crop = $state({ ...FULL_FRAME });
    let isCropping = $state(false);
    let isStraightening = $state(false);
    // The crop tool's aspect lock, kept from one image to the next.
    let aspectLock: AspectLock = $state({ ...FREE_ASPECT });
    let aspectMenuOpen = $state(false);
    let customAspectOpen = $state(false);
    let lockedAspect = $derived.by(() =>
        imagePayload ? aspectRatio(aspectLock, imagePayload.fullWidth, imagePayload.fullHeight) : null,
    );

    // Canvas pixels per image pixel, or null while the image is fitted to its
    // slot. The crop tool always shows the image fitted.
    let zoom: number | null = $state(null);
    let zoomable = $state(false);

    function toggleZoom() {
        zoom = zoom === null ? 1 : null;
    }

    // The adjustments have nothing to act on without an image, and are set
    // aside while the crop tool has the preview.
    let sidebarInactive = $derived(!imagePayload || isCropping);

    // An export menu left open when the footer goes inactive would stay up,
    // dimmed and unreachable.
    $effect(() => {
        if (sidebarInactive) exportMenuOpen = false;
    });

    // Any press in the sidebar but on the loupe itself settles where it is.
    function endLoupeOnPress(e: PointerEvent) {
        if (!loupe.picking) return;
        if (e.target instanceof Element && e.target.closest("[data-loupe]")) return;
        loupe.picking = false;
    }

    function enterCrop() {
        presets.open = false;
        zoom = null;
        draftCrop = { ...crop };
        isCropping = true;
        holdAspectOnEntry();
    }

    // An uncropped frame takes on the lock. A crop already drawn to another
    // shape is kept, and the lock lets go.
    function holdAspectOnEntry() {
        if (!imagePayload || aspectLock.preset === "free") return;
        const { fullWidth, fullHeight } = imagePayload;
        const shape = cropAspect(draftCrop, fullWidth, fullHeight);
        const uncropped = draftCrop.w === 1 && draftCrop.h === 1;
        const lock = { ...aspectLock, portrait: uncropped ? shape < 1 : aspectLock.portrait };
        const ratio = aspectRatio(lock, fullWidth, fullHeight)!;
        if (aspectsMatch(shape, ratio)) aspectLock = lock;
        else if (aspectsMatch(shape, 1 / ratio)) aspectLock = { ...lock, portrait: !lock.portrait };
        else if (uncropped) setAspect(lock);
        else aspectLock = { ...FREE_ASPECT };
    }

    // A newly chosen ratio follows the box's current orientation; the draft is
    // then refitted to it.
    function setAspect(lock: AspectLock) {
        if (!imagePayload) return;
        const { fullWidth, fullHeight } = imagePayload;
        if (lock.preset !== aspectLock.preset && lock.preset !== "custom") {
            lock = { ...lock, portrait: cropAspect(draftCrop, fullWidth, fullHeight) < 1 };
        }
        aspectLock = lock;
        const ratio = aspectRatio(lock, fullWidth, fullHeight);
        if (ratio === null || aspectsMatch(cropAspect(draftCrop, fullWidth, fullHeight), ratio)) return;
        draftCrop = cropWithAspect(draftCrop, ratio, fullWidth, fullHeight);
    }

    // An entered ratio stands the way it was typed.
    function setCustomAspect(w: number, h: number) {
        setAspect({ preset: "custom", portrait: h > w, custom: [w, h] });
    }

    function resetDraft() {
        draftCrop = { ...FULL_FRAME };
        aspectLock = { ...FREE_ASPECT };
    }

    function applyCrop() {
        isCropping = false;
        isStraightening = false;
        if (cropsEqual(draftCrop, crop)) return;
        zoom = null;
        commit({ type: "crop", oldValue: { ...crop }, newValue: { ...draftCrop } });
        crop = { ...draftCrop };
    }

    function cancelCrop() {
        isCropping = false;
        isStraightening = false;
    }

    function turnDraft(clockwise: boolean) {
        draftCrop = turnCrop(draftCrop, clockwise);
        aspectLock = { ...aspectLock, portrait: !aspectLock.portrait };
    }

    function flipDraft(horizontal: boolean) {
        draftCrop = flipCrop(draftCrop, horizontal);
    }

    // Outside the crop tool a flip lands on the applied crop as its own step.
    function flipShortcut(horizontal: boolean) {
        if (!imagePayload) return;
        if (isCropping) return flipDraft(horizontal);
        const flipped = flipCrop(crop, horizontal);
        commit({ type: "crop", oldValue: { ...crop }, newValue: flipped });
        crop = flipped;
    }

    function toggleCrop() {
        if (!imagePayload) return;
        if (isCropping) applyCrop();
        else enterCrop();
    }

    // The darkroom's per-channel histograms follow the crop, so a film border
    // left outside it stops weighing on the black and white points.
    let inputHistograms: ChannelHistograms | null = $state(null);
    let histogramRequest = 0;

    $effect(() => {
        const payload = imagePayload;
        const kept = { ...crop };
        const request = ++histogramRequest;

        if (!payload) {
            inputHistograms = null;
        } else if (isFullFrame(kept)) {
            inputHistograms = { r: payload.histR, g: payload.histG, b: payload.histB };
        } else {
            cropHistograms(kept)
                .then((h) => {
                    if (request === histogramRequest) inputHistograms = h;
                })
                .catch((e) => console.error("Failed to bin the crop:", e));
        }
    });

    let isExportPending = $state(false);
    let showExportModal = $state(false);
    let exportMenuOpen = $state(false);
    let exportSettings = $state<ExportSettings>({ ...defaultExportSettings });
    let footerEl: HTMLDivElement | null = $state(null);

    // What a soft proof proofs against is whatever the export is set to write,
    // kept here rather than in the menu so it holds while the menu is closed.
    $effect(() => {
        proofing.target = exportSettings.outputSpace;
    });

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
        if (isCropping) applyCrop();
        try {
            await exportImage(sliders, curveLut, colorSpaceDebug.toneMap, crop, exportSettings);
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
        // Numbers travel to their new values the way the colour-mode toggle
        // moves them; anything else lands at once.
        setSliders: (values) => {
            const numeric: Record<string, number> = {};
            for (const [key, val] of Object.entries(values)) {
                if (typeof val === "number") numeric[key] = val;
                else (sliders as unknown as Record<string, typeof val>)[key] = val;
            }
            animateObject(
                sliders as unknown as Record<string, number>,
                numeric,
            );
        },
        setCurves: (next) => {
            curves = cloneCurves(next);
        },
        setCrop: (value) => {
            crop = { ...value };
            cancelCrop();
        },
    };

    function commit(action: Action) {
        history.push(action);
    }

    // Undo or redo inside the crop tool first throws away an unapplied drag;
    // only with nothing to throw away does it reach the history.
    function leaveCropForHistory(): boolean {
        if (!isCropping) return false;
        const discarded = !cropsEqual(draftCrop, crop);
        cancelCrop();
        return discarded;
    }

    // Stepping through the history shows the mode and page whose controls it
    // changes. Controls both modes show stay in the mode the user is in.
    function showControlsFor(a: Action) {
        const mode = actionMode(a) ?? (isDarkroom ? "darkroom" : "lighter-room");
        const darkroom = mode === "darkroom";
        const page = actionPage(a, mode, darkroom ? activePageDarkroom : activePageNormal);
        if (page !== null) jumpToPage(darkroom, page);
        if (darkroom !== isDarkroom) handleModeToggle();
    }

    function undo() {
        if (leaveCropForHistory()) return;
        const a = history.undo();
        if (!a) return;
        showControlsFor(a);
        undoAction(a, stateAccessors);
    }
    function redo() {
        if (leaveCropForHistory()) return;
        const a = history.redo();
        if (!a) return;
        showControlsFor(a);
        applyAction(a, stateAccessors);
    }

    // ===== Adjustments clipboard =====

    /* In B&W the colour keys are pinned to their overrides, so an incoming set
       puts its colour values aside for the toggle back to colour instead of
       applying them. The whole set lands as one history step, with any curves
       it carries. */
    function applySliderSet(values: Partial<Sliders>, curveValues: Partial<Curves> = {}) {
        const next: Partial<Sliders> = { ...values };

        if (!isRgb) {
            const held: Partial<Sliders> = { ...(savedColorValues ?? {}) };
            for (const key of Object.keys(overridesBW) as (keyof Sliders)[]) {
                if (key in next) {
                    (held as any)[key] = next[key];
                    delete next[key];
                }
            }
            savedColorValues = held;
        }

        const oldValues: Partial<Sliders> = {};
        for (const key of Object.keys(next) as (keyof Sliders)[]) {
            (oldValues as any)[key] = sliders[key];
        }

        const oldCurves = cloneCurves($state.snapshot(curves) as Curves);
        const newCurves = cloneCurves({ ...oldCurves, ...curveValues });
        const curvesChange = curvesEqual(oldCurves, newCurves)
            ? {}
            : { oldCurves, newCurves };

        commit({ type: "sliders", oldValues, newValues: next, ...curvesChange });
        stateAccessors.setSliders(next);
        if (curvesChange.newCurves) stateAccessors.setCurves(curvesChange.newCurves);
    }

    // All three act on the sliders of the mode the user is looking at, which
    // includes the ones both modes share.
    let activeKeys = $derived(modeKeys(isDarkroom));

    async function handleCopyAdjustments() {
        try {
            await copyAdjustments(
                sliders,
                activeKeys,
                isDarkroom ? undefined : ($state.snapshot(curves) as Curves),
            );
        } catch (e) {
            console.error("Failed to copy adjustments:", e);
        }
    }

    // Pasting and resetting change the sliders, so they wait while the
    // sidebar is set aside. Copying only reads them.
    async function handlePasteAdjustments() {
        if (sidebarInactive) return;
        try {
            const pasted = await readAdjustments(activeKeys, !isDarkroom);
            applySliderSet(pasted.sliders, pasted.curves);
        } catch (e) {
            console.error("Failed to paste adjustments:", e);
        }
    }

    // Always the colour defaults: in B&W the overridden keys are diverted to
    // the saved colour values, leaving the mode itself untouched.
    function resetKeys(keys: readonly (keyof Sliders)[], curveValues: Partial<Curves> = {}) {
        const next: Partial<Sliders> = {};
        for (const key of keys) {
            (next as any)[key] = defaultSlidersRGB[key];
        }
        applySliderSet(next, curveValues);
    }

    // Lands as one history step and shows the levels page it changed.
    function handleAutoLevels() {
        if (!isDarkroom || sidebarInactive || !inputHistograms) return;
        const levels = autoLevels(inputHistograms);
        if (Object.keys(levels).length === 0) return;
        applySliderSet(levels);
        const page = actionPage(
            { type: "sliders", oldValues: {}, newValues: levels },
            "darkroom",
            activePageDarkroom,
        );
        if (page !== null) scrollToPage(page);
    }

    function handleInvertShortcut() {
        if (!isDarkroom || sidebarInactive) return;
        sliders.invert = !sliders.invert;
        handleInvertToggle(sliders.invert);
    }

    function handleResetAdjustments() {
        if (sidebarInactive) return;
        resetKeys(activeKeys, isDarkroom ? {} : defaultCurves());
    }
    const handleResetColourMixer = () => resetKeys(HSL_KEYS);

    /* The tolerance is there because animated values land on their target
       through a lerp, which need not be bit-exact. */
    function isAtDefault(key: keyof Sliders) {
        const def = defaultSlidersRGB[key];
        const val = sliders[key];
        if (typeof def === "number" && typeof val === "number") {
            return Math.abs(val - def) <= SLIDER_TOLERANCE;
        }
        return val === def;
    }

    let isColourMixerIdentity = $derived(HSL_KEYS.every(isAtDefault));

    let isCurvesIdentity = $derived(curvesEqual(curves, defaultCurves()));

    // ===== Presets =====

    let hoveredPreset = $state<Preset | null>(null);
    let previewPreset = $derived(presets.open && !sidebarInactive ? hoveredPreset : null);

    $effect(() => {
        if (!presets.open) hoveredPreset = null;
    });

    /* What the preview shows: the committed state with the hovered preset laid
       over it, minus what applying it in B&W would divert. */
    let shownSliders: Sliders = $derived.by(() => {
        if (!previewPreset) return sliders;
        const next = { ...sliders };
        for (const [key, value] of Object.entries(previewPreset.sliders)) {
            if (!isRgb && key in overridesBW) continue;
            (next as any)[key] = value;
        }
        return next;
    });

    let shownCurveLut = $derived(
        previewPreset?.curves
            ? bakeCurveLut({ ...curves, ...previewPreset.curves }, isRgb)
            : curveLut,
    );

    function applyPreset(preset: Preset) {
        if (sidebarInactive) return;
        applySliderSet({ ...preset.sliders }, preset.curves ?? {});
    }

    type PresetDialog = { editing: Preset | null; name: string; fields: Set<string> };
    let presetDialog = $state<PresetDialog | null>(null);

    function currentCurves() {
        return cloneCurves($state.snapshot(curves) as Curves);
    }

    // A new preset starts from whatever has been changed, or from the whole
    // mode when nothing has.
    function openNewPreset() {
        let fields = changedFieldIds(sliders, curves);
        if (fields.size === 0) fields = modeFieldIds(isDarkroom ? "darkroom" : "lighter-room");
        presetDialog = { editing: null, name: "", fields };
    }

    function openEditPreset(preset: Preset) {
        presetDialog = { editing: preset, name: preset.name, fields: presetFieldIds(preset) };
    }

    function savePresetDialog(name: string, fields: Set<string>, overwrite: boolean) {
        const dialog = presetDialog;
        presetDialog = null;
        if (!dialog) return;

        const current = captureFields(fields, sliders, currentCurves());
        const stored = dialog.editing;
        if (!stored) {
            presets.add({ name, ...current });
            return;
        }
        if (overwrite) {
            presets.update(stored.id, { name, ...current });
            return;
        }

        presets.update(stored.id, { name, ...revisePreset(stored, current, fields) });
    }

    async function exportPreset(preset: Preset) {
        try {
            await presets.exportFile(preset);
        } catch (e) {
            console.error("Failed to export preset:", e);
        }
    }

    async function importPresets() {
        try {
            const failed = await presets.importFiles();
            if (failed > 0) console.warn(`${failed} preset file(s) could not be read`);
        } catch (e) {
            console.error("Failed to import presets:", e);
        }
    }

    function deletePreset(preset: Preset) {
        if (hoveredPreset?.id === preset.id) hoveredPreset = null;
        presets.remove(preset.id);
    }

    // Whether a reset would change anything, judged on the same keys it writes.
    let isIdentity = $derived(
        (isDarkroom || isCurvesIdentity) &&
        activeKeys.every(
            (key) => ((!isRgb || isColorAnimating) && key in overridesBW) || isAtDefault(key),
        ),
    );

    // Frame stats start visible in a dev run and hidden in a release build, so
    // the numbers are there while working without shipping to a user. A release
    // build still needs to be measurable — turning the debug tools on in
    // settings brings F1/F2 back.
    debugStats.enabled = import.meta.env.DEV;

    const handleKeydown = createKeydownHandler({
        handlers: {
            pagePrev: () => movePage(false),
            pageNext: () => movePage(true),
            undo: undo,
            redo: redo,
            changeMode: handleModeToggle,
            openImage: handleOpenImage,
            copyAdjustments: handleCopyAdjustments,
            pasteAdjustments: handlePasteAdjustments,
            resetAdjustments: handleResetAdjustments,
            toggleCrop: toggleCrop,
            flipHorizontal: () => flipShortcut(true),
            flipVertical: () => flipShortcut(false),
            autoLevels: handleAutoLevels,
            toggleInvert: handleInvertShortcut,
            // Both diagnostics hang off the settings master switch, so neither
            // shortcut does anything until the debug tools are turned on.
            toggleDebugStats: () => {
                if (!settings.debugTools) return;
                debugStats.enabled = !debugStats.enabled;
                if (!debugStats.enabled) debugStats.clear();
            },
            toggleHistogram: () => {
                if (!settings.debugTools) return;
                debugStats.histogramEnabled = !debugStats.histogramEnabled;
                // The point of stopping it is to read the difference, so start
                // the comparison from a clean window rather than a mixed
                // average.
                debugStats.clear();
            },
            toggleColorSpace: () => {
                if (!settings.debugTools) return;
                colorSpaceDebug.toggle();
            },
            toggleSettings: () => settings.toggle(),
        },
        closeSettings: () => settings.close(),
        isSettingsOpen: () => settings.open,
    });

    // Enter and Escape belong to the crop tool while it is open, unless a field
    // or the export menu has the keyboard.
    function handleWindowKeydown(e: KeyboardEvent) {
        // The preset dialog takes the keyboard while it is up.
        if (presetDialog) {
            if (e.key === "Escape") {
                presetDialog = null;
                e.preventDefault();
            }
            return;
        }
        // So does the custom ratio dialog, which answers its own keys.
        if (customAspectOpen) return;
        if (loupe.picking && e.key === "Escape" && !settings.open && !isTextField(e.target)) {
            loupe.picking = false;
            e.preventDefault();
            return;
        }
        const ownsKey =
            isCropping &&
            !settings.open &&
            !exportMenuOpen &&
            !isTextField(e.target);
        if (ownsKey && (e.key === "Enter" || e.key === "Escape")) {
            if (e.key === "Enter") applyCrop();
            else if (aspectMenuOpen) aspectMenuOpen = false;
            else if (isStraightening) isStraightening = false;
            else cancelCrop();
            e.preventDefault();
            (document.activeElement as HTMLElement)?.blur();
            return;
        }
        handleKeydown(e);
    }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<TitleBar
    {undo}
    {redo}
    open={handleOpenImage}
    copyAdjustments={handleCopyAdjustments}
    pasteAdjustments={handlePasteAdjustments}
    resetAdjustments={handleResetAdjustments}
    openSettings={(section) => settings.show(section)}
></TitleBar>
<!-- Shared by both modes, so the two sets of controls stay one and the same. -->
{#snippet unsharpMask()}
    <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
        <span class="section-title pb-1.5"
            >Unsharp Mask</span
        >
        <Loupe />
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
{/snippet}

<!-- Full-window canvas background. The image is centred on .canvas-slot but is
     free to spill out of it in every direction once zoomed; the toolbar and
     side bar sit on a higher layer and simply paint over the overflow. -->
<div class="canvas-backdrop"></div>
<div class="app-shell flex absolute flex-row gap-3">
    <div class="chrome-scrim" class:shifted={presets.open}>
        <div class="scrim-fill"></div>
    </div>
    <div class="presets-column shrink-0" class:open={presets.open} inert={!presets.open}>
        <div class="presets-slide h-full">
            <PresetsPanel
                list={presets.list}
                canApply={!sidebarInactive}
                onapply={applyPreset}
                onhover={(preset) => (hoveredPreset = preset)}
                onadd={openNewPreset}
                onimport={importPresets}
                onedit={openEditPreset}
                onexport={exportPreset}
                ondelete={deletePreset}
            />
        </div>
    </div>
    <div class="toolbar shrink-0">
        <div
            class="toolbar-tools relative flex flex-col items-center"
            class:dimmed={!imagePayload}
            inert={!imagePayload}
        >
            <CropButton active={isCropping} ontoggle={toggleCrop} />
            <PresetsButton
                active={presets.open}
                disabled={isCropping}
                ontoggle={() => presets.toggle()}
            />
            <CropTools
                visible={isCropping}
                bind:straightening={isStraightening}
                bind:aspectMenuOpen
                aspect={aspectLock}
                aspectRatio={lockedAspect}
                canReset={!isFullFrame(draftCrop)}
                onturn={turnDraft}
                onflip={flipDraft}
                onaspect={setAspect}
                oncustom={() => (customAspectOpen = true)}
                onreset={resetDraft}
            />
        </div>
        <div
            class="toolbar-zoom absolute flex justify-center"
            class:dimmed={!imagePayload}
            inert={!imagePayload}
        >
            <ZoomButton
                fitted={zoom === null}
                disabled={isCropping || loupe.picking || (zoom === null && !zoomable)}
                ontoggle={toggleZoom}
            />
        </div>
    </div>
    <!-- Its own column, so opening it shifts the image rather than covering it. -->
    {#if settings.debugTools && colorSpaceDebug.enabled}
        <div class="colour-space-bar shrink-0">
            <ColorSpacePanel sliders={shownSliders} curveLut={shownCurveLut} {imagePayload} />
        </div>
    {/if}
    <div class="canvas-slot relative flex-1">
        <div class="canvas-center absolute flex items-center justify-center">
            <PreviewImageCanvas
                sliders={shownSliders}
                curveLut={shownCurveLut}
                {imagePayload}
                crop={isCropping ? draftCrop : crop}
                showWhole={isCropping}
                bind:zoom
                bind:zoomable
            >
                {#snippet overlay()}
                    {#if isCropping && imagePayload}
                        <CropOverlay
                            bind:crop={draftCrop}
                            bind:straightening={isStraightening}
                            aspect={lockedAspect}
                            imageWidth={imagePayload.fullWidth}
                            imageHeight={imagePayload.fullHeight}
                        />
                    {/if}
                {/snippet}
            </PreviewImageCanvas>
        </div>
        {#if settings.debugTools && debugStats.enabled}
            <DebugStatsOverlay />
        {/if}
    </div>
    <div class="side-bar flex w-[23%] flex-col gap-3" onpointerdowncapture={endLoupeOnPress}>
        <div
            class="histogram-panel glass shrink-0 rounded-[12px]"
            class:dimmed={settings.debugTools && !debugStats.histogramEnabled}
        >
            <div
                class="histogram-container m-1.25"
                class:dimmed={sidebarInactive}
                inert={sidebarInactive}
            >
                <Histogram
                    sliders={shownSliders}
                    curveLut={shownCurveLut}
                    {imagePayload}
                    {crop}
                    curveCanvas={curveHistogramCanvas}
                    masterCanvas={masterHistogramCanvas}
                    curveChannel={isRgb ? curveChannel : "master"}
                />
            </div>
        </div>
        <div class="tools-panel glass flex rounded-[12px] flex-1 flex-col">
            <div
                class="quick-actions flex items-center flex-row gap-2 ml-3 mr-3 mt-2.5"
                class:dimmed={exportMenuOpen || sidebarInactive}
                inert={sidebarInactive}
            >
                <ColorModeToggle bind:isRgb onToggle={handleColorModeToggle} />
                <!-- Stays mounted and fades. It is the last item in a
                     left-aligned row, so holding its width costs no layout —
                     and a plain opacity crossfade never touches layout at all,
                     where `slide` animated width/padding/margin and reflowed
                     the row on every frame. -->
                <div class="invert-slot flex items-center gap-2" class:shown={isDarkroom}>
                    <InvertToggle bind:isInverted={sliders.invert} onToggle={handleInvertToggle} />
                    <AutoLevelsButton disabled={!inputHistograms} onauto={handleAutoLevels} />
                </div>
                <!-- Pushed to the far end of the row, opposite the toggles. -->
                <div class="ml-auto">
                    <RevertButton
                        visible={!isIdentity}
                        onrevert={handleResetAdjustments}
                    />
                </div>
            </div>
            <div
                class="controls-section flex relative flex-1 flex-col"
                class:dimmed={exportMenuOpen || sidebarInactive}
                inert={sidebarInactive}
            >
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
                            onpointerdown={handlePagerSideButton}
                            onauxclick={handlePagerSideButton}
                            role="group"
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
                                {@render unsharpMask()}
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
                            <!-- Curves, HSL & Colour Mixer Panel -->
                            <div class="controls-page">
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <CurveEditor
                                        bind:curves
                                        bind:channel={curveChannel}
                                        bind:histogramCanvas={curveHistogramCanvas}
                                        {isRgb}
                                        onCommit={(oldCurves, newCurves) =>
                                            commit({ type: "curves", oldCurves, newCurves })}
                                    >
                                        {#snippet title()}
                                            <span class="section-title">Curves</span>
                                        {/snippet}
                                    </CurveEditor>
                                </div>
                                {#if isRgb}
                                    <div
                                        class="slider-section flex flex-col mt-4 ml-4 mr-3"
                                        transition:slide={{ duration: 200 }}
                                    >
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
                                    </div>
                                    <div
                                        class="slider-section flex flex-col mt-4 ml-4 mr-3"
                                        transition:slide={{ duration: 200 }}
                                    >
                                        <div class="flex flex-row items-center justify-between pb-1.5 pr-1.5">
                                            <span class="section-title">Colour Mixer</span>
                                            <RevertButton
                                                visible={!isColourMixerIdentity}
                                                label="Reset Colour Mixer"
                                                onrevert={handleResetColourMixer}
                                            />
                                        </div>
                                        <HslBandGroup
                                            bind:sliders
                                            onCommit={(key, oldVal, newVal) =>
                                                commit({
                                                    type: "slider",
                                                    key,
                                                    oldValue: oldVal,
                                                    newValue: newVal,
                                                })}
                                        ></HslBandGroup>
                                    </div>
                                {/if}
                                <div class="flex h-7.5"></div>
                            </div>
                        </div>
                    {/if}
                    {#if isDarkroom}
                        <div
                            class="controls-pager absolute flex z-1"
                            bind:this={pagerElDarkroom}
                            onscroll={handleDarkroomPagerScroll}
                            onwheel={handlePagerWheel}
                            onwheelcapture={handlePagerWheel}
                            onpointerdown={handlePagerSideButton}
                            onauxclick={handlePagerSideButton}
                            role="group"
                            transition:slideX={{ direction: 1 }}
                        >
                            <!-- Darkroom Sharpness Panel -->
                            <div class="controls-page">
                                {@render unsharpMask()}
                                <div class="flex h-7.5"></div>
                            </div>
                            <!-- Input Levels Panel -->
                            <div class="controls-page">
                                <div class="slider-section flex flex-col mt-4 ml-4 mr-3">
                                    <TripleValSliderHistGroup
                                        name="Red Input"
                                        bind:valueA={sliders.redBlackPoint}
                                        bind:valueB={sliders.redGamma}
                                        bind:valueC={sliders.redWhitePoint}
                                        onCommitA={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "redBlackPoint", oldValue, newValue })}
                                        onCommitB={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "redGamma", oldValue, newValue })}
                                        onCommitC={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "redWhitePoint", oldValue, newValue })}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={inputHistograms?.r ?? null}
                                        histogramChannel="r"
                                    ></TripleValSliderHistGroup>
                                    <TripleValSliderHistGroup
                                        name="Green Input"
                                        bind:valueA={sliders.greenBlackPoint}
                                        bind:valueB={sliders.greenGamma}
                                        bind:valueC={sliders.greenWhitePoint}
                                        onCommitA={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "greenBlackPoint", oldValue, newValue })}
                                        onCommitB={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "greenGamma", oldValue, newValue })}
                                        onCommitC={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "greenWhitePoint", oldValue, newValue })}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={inputHistograms?.g ?? null}
                                        histogramChannel="g"
                                    ></TripleValSliderHistGroup>
                                    <TripleValSliderHistGroup
                                        name="Blue Input"
                                        bind:valueA={sliders.blueBlackPoint}
                                        bind:valueB={sliders.blueGamma}
                                        bind:valueC={sliders.blueWhitePoint}
                                        onCommitA={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "blueBlackPoint", oldValue, newValue })}
                                        onCommitB={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "blueGamma", oldValue, newValue })}
                                        onCommitC={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "blueWhitePoint", oldValue, newValue })}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        histogramData={inputHistograms?.b ?? null}
                                        histogramChannel="b"
                                    ></TripleValSliderHistGroup>
                                    <TripleValSliderHistGroup
                                        name="RGB Input"
                                        bind:valueA={sliders.masterBlackPoint}
                                        bind:valueB={sliders.masterGamma}
                                        bind:valueC={sliders.masterWhitePoint}
                                        onCommitA={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "masterBlackPoint", oldValue, newValue })}
                                        onCommitB={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "masterGamma", oldValue, newValue })}
                                        onCommitC={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "masterWhitePoint", oldValue, newValue })}
                                        defaultValueA={0}
                                        defaultValueB={1}
                                        defaultValueC={255}
                                        min={0}
                                        max={255}
                                        minB={0.1}
                                        maxB={2}
                                        innerColor={"#430000"}
                                        outerColor={"#000000"}
                                        liveHistogram
                                        bind:histogramCanvas={masterHistogramCanvas}
                                    ></TripleValSliderHistGroup>
                                    <DoubleValSliderGroup
                                        name="RGB Output"
                                        bind:valueA={sliders.rgbOutputBlack}
                                        bind:valueB={sliders.rgbOutputWhite}
                                        onCommitA={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "rgbOutputBlack", oldValue, newValue })}
                                        onCommitB={(oldValue, newValue) =>
                                            commit({ type: "slider", key: "rgbOutputWhite", oldValue, newValue })}
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
            <div
                class="footer-item absolute"
                style="left: 10px;"
                class:dimmed={sidebarInactive}
                inert={sidebarInactive}
            >
                <ExportButton onexport={handleExport} bind:menuOpen={exportMenuOpen} settings={exportSettings}></ExportButton>
            </div>
            <div
                class="footer-item page-dots-wrapper relative flex justify-center items-center"
                class:dimmed={sidebarInactive}
                inert={sidebarInactive}
            >
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
                        {#each Array(3) as _, i}
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
                        {#each Array(2) as _, i}
                            <button
                                class="dot size-1.5 rounded-full border-0 cursor-pointer p-0"
                                class:active={activePageDarkroom === i}
                                onclick={() => scrollToPage(i)}
                                aria-label="Go to darkroom page {i + 1}"
                                aria-current={activePageDarkroom === i
                                    ? "step"
                                    : undefined}
                            ></button>
                        {/each}
                    </div>
            </div>
            <div
                class="footer-item absolute"
                style="right: 10px;"
                class:dimmed={sidebarInactive}
                inert={sidebarInactive}
            >
                <ModeToggle bind:isDarkroom onModeToggle={handleModeToggle}
                ></ModeToggle>
            </div>
        </div>
    </div>
</div>

<ExportModal visible={showExportModal}></ExportModal>
<CustomAspectModal
    bind:open={customAspectOpen}
    initial={aspectLock.custom ?? [1, 1]}
    onapply={setCustomAspect}
/>

{#if presetDialog}
    <PresetModal
        editing={presetDialog.editing !== null}
        initialName={presetDialog.name}
        initialFields={presetDialog.fields}
        onsave={savePresetDialog}
        oncancel={() => (presetDialog = null)}
    />
{/if}

<style>
    .canvas-backdrop {
        position: fixed;
        inset: 0;
        background: var(--canvasBackdrop);
        transition: background-color 0.12s ease;
        z-index: 0;
    }

    .app-shell {
        --presets-width: 260px;
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
        /* Breathing room so a fitted image never touches the panels. The left
           is shorter because the toolbar's buttons sit inside its column, so
           this leaves the same gap to them as to the side bar. */
        inset: 36px 36px 36px 29px;
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

    /* The one gradient the window chrome sits on: down the left edge under the
       toolbar and across the top under the title bar, turning the corner
       between them on a rounded inner edge. Two copies are stacked, a dense
       band and a wide faint tail, and both ramp in on an eased curve, so
       neither has a visible line where it starts. */
    .chrome-scrim {
        position: fixed;
        inset: 0;
        z-index: 5;
        pointer-events: none;
        transition: transform 0.35s cubic-bezier(0.2, 0, 0, 1);
    }

    /* Follows the toolbar when the presets column opens: the panel width plus
       the gap either side of it. */
    .chrome-scrim.shifted {
        transform: translateX(calc(var(--presets-width) + 24px));
    }

    /* What the shift uncovers at the window edge, held at the darkness both
       bands reach there, so the presets sit under the same cast. */
    .scrim-fill {
        position: absolute;
        top: 0;
        bottom: 0;
        right: 100%;
        width: calc(var(--presets-width) + 24px);
        background: black;
        opacity: calc(
            var(--scrimStrength) * 1.35 - var(--scrimStrength) * var(--scrimStrength) * 0.35
        );
        transition: opacity 0.5s ease;
    }

    .chrome-scrim::before,
    .chrome-scrim::after {
        content: "";
        position: absolute;
        inset: 0;
        /* Smoothstep, from each band's inner edge out to the window edge. */
        --ramp: transparent, rgba(0, 0, 0, 0.06) 15%, rgba(0, 0, 0, 0.22) 30%,
            rgba(0, 0, 0, 0.42) 45%, rgba(0, 0, 0, 0.65) 60%,
            rgba(0, 0, 0, 0.84) 75%, rgba(0, 0, 0, 0.97) 90%, black;
        /* The corner tile runs the same ramp, pushed out past the inner radius
           (the first 31.25% of the tile), so it meets both bands exactly. */
        background:
            radial-gradient(
                    var(--reach-x) var(--reach-y) at var(--reach-x)
                        var(--reach-y),
                    transparent 31.25%,
                    rgba(0, 0, 0, 0.06) 41.56%,
                    rgba(0, 0, 0, 0.22) 51.88%,
                    rgba(0, 0, 0, 0.42) 62.19%,
                    rgba(0, 0, 0, 0.65) 72.5%,
                    rgba(0, 0, 0, 0.84) 82.81%,
                    rgba(0, 0, 0, 0.97) 93.13%,
                    black
                )
                0 0 / var(--reach-x) var(--reach-y) no-repeat,
            linear-gradient(to top, var(--ramp)) var(--reach-x) 0 /
                calc(100% - var(--reach-x)) var(--top) no-repeat,
            linear-gradient(to left, var(--ramp)) 0 var(--reach-y) /
                var(--left) calc(100% - var(--reach-y)) no-repeat;
        transition: opacity 0.5s ease;
    }

    /* Each band's depth, and where the corner tile hands over to it: the depth
       plus an inner radius in proportion to it. The hand-overs stay on
       multiples of 4px so the seams land on whole device pixels at every
       common display scale. */
    .chrome-scrim::before {
        --top: 55px;
        --left: 68.75px;
        --reach-x: 100px;
        --reach-y: 80px;
        opacity: var(--scrimStrength);
    }

    .chrome-scrim::after {
        --top: 143px;
        --left: 178.75px;
        --reach-x: 260px;
        --reach-y: 208px;
        opacity: calc(var(--scrimStrength) * 0.35);
    }

    /* The icon column. It bleeds past the shell's insets to the window's left,
       top and bottom edges so its buttons sit on the scrim's left band. */
    .toolbar {
        position: relative;
        margin: -44px 0 -12px -12px;
        min-width: 48px;
        z-index: 10;
        pointer-events: auto;
    }

    .footer-item {
        transition: opacity 0.2s ease;
    }

    /* Opens as a column of its own, pushing the toolbar right and re-centring
       the image in what is left. Closed, its negative margin cancels the
       shell's gap; open, it also covers the toolbar's bleed to the edge. */
    .presets-column {
        width: 0;
        margin-right: -12px;
        position: relative;
        z-index: 10;
        pointer-events: auto;
        transition:
            width 0.35s cubic-bezier(0.2, 0, 0, 1),
            margin-right 0.35s cubic-bezier(0.2, 0, 0, 1);
    }

    .presets-column.open {
        width: var(--presets-width);
        margin-right: 12px;
    }

    /* The panel keeps its width and slides in with the toolbar rather than
       being squeezed, so its list never reflows mid-animation. */
    .presets-slide {
        /* Laps 4px into the shell's edge inset, so the names sit 18px in. */
        width: calc(var(--presets-width) + 4px);
        margin-left: -4px;
        transform: translateX(calc(-100% - 24px));
        transition: transform 0.35s cubic-bezier(0.2, 0, 0, 1);
    }

    .presets-column.open .presets-slide {
        transform: translateX(0);
    }

    .toolbar-tools {
        transition: opacity 0.2s ease;
        padding-top: 50px;
        padding-right: 6px;
        gap: 4px;
        z-index: 1;
    }

    /* At the foot of the icon column. The button's chip, inset 3px inside its
       box, sits as far from the window's bottom edge as from its left. */
    .toolbar-zoom {
        left: 0;
        right: 6px;
        bottom: 0;
        z-index: 1;
        transition: opacity 0.2s ease;
    }

    /* Lifted above the canvas like the side bar. The padding leaves room for
       the panels' drop shadows, which the scroll container would clip. */
    .colour-space-bar {
        width: 300px;
        padding: 0 28px 24px 0;
        position: relative;
        z-index: 10;
        pointer-events: auto;
        overflow-y: auto;
        scrollbar-width: none;
    }

    .colour-space-bar::-webkit-scrollbar {
        display: none;
    }

    .side-bar {
        min-width: 300px;
        /* A faint lift so labels and values hold up over a bright image. */
        text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
        position: relative;
        z-index: 10;
        pointer-events: auto;
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
        font-family: var(--font-ui);
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
