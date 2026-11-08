// $lib/editor/editor.svelte.ts
//
// The open image and everything done to it, with the tools that act on it.
// Held outside any component, so the editor keeps its state while out of view.

import { untrack } from "svelte";
import { listen } from "@tauri-apps/api/event";

import { presets } from "$lib/presets/presets.svelte";
import {
    type Preset,
    captureFields,
    changedFieldIds,
    modeFieldIds,
    presetFieldIds,
    revisePreset,
} from "$lib/presets/presetDocument";
import { retouchTool } from "$lib/retouch/retouchTool.svelte";
import { loupe } from "$lib/gpu/loupe.svelte";
import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
import { debugStats } from "$lib/gpu/debugStats.svelte";
import { proofing } from "$lib/gpu/proofing.svelte";
import { history } from "$lib/history/history.svelte";
import { applyAction, undoAction, type StateAccessors } from "$lib/history/historyDispatch";
import { actionMode, actionPage } from "$lib/history/actionControls";
import type { Action } from "$lib/types/historyActions";
import type { EditHistory } from "$lib/types/editHistory";
import { type Retouch, type RetouchMode, retouchEqual } from "$lib/types/retouch";
import {
    type Sliders,
    defaultSlidersRGB,
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
import { type ImageMetadata } from "$lib/types/imageMetadata";
import { type ImageEdit, editFitsSource, newImageEdit } from "$lib/types/imageEdit";
import { type Crop, FULL_FRAME, cropsEqual, isFullFrame, turnCrop, flipCrop } from "$lib/types/crop";
import {
    type AspectLock,
    FREE_ASPECT,
    aspectRatio,
    aspectsMatch,
    cropAspect,
    cropWithAspect,
} from "$lib/types/cropAspect";
import { type ExportSettings, defaultExportSettings } from "$lib/types/exportSettings";
import { cropHistograms, type ChannelHistograms } from "$lib/utils/cropHistograms";
import { autoLevels } from "$lib/utils/autoLevels";
import { fetchImageMetadata, openImage, openImagePath } from "$lib/utils/openImage";
import { copyAdjustments, readAdjustments } from "$lib/utils/adjustmentsClipboard";
import { exportImage } from "$lib/utils/exportImage";
import { animateObject } from "$lib/utils/animateObject";

// How long the control pagers take to slide across on a mode change.
export const MODE_SLIDE_MS = 500;

/* The colour keys are in transit for as long as the toggle animates them, so
   anything reading them has to wait rather than judge them mid-flight. */
const COLOR_ANIM_MS = 200;

type HistogramInput = { crop: Crop; strokes: Retouch[] };
type PresetDialog = { editing: Preset | null; name: string; fields: Set<string> };

class Editor {
    // ===== Image =====

    /** Everything done to the open image, as one document. */
    edit = $state<ImageEdit>(newImageEdit(0, 0));

    imagePayload = $state<ImagePayload | null>(null);

    /** How the open image was shot, once the backend has said. */
    metadata = $state.raw<ImageMetadata | null>(null);

    /** Whether the info panel is open. It shares its column with the presets. */
    infoOpen = $state(false);

    /** The library's id for the open image, or null when it was opened from outside it. */
    libraryImageId = $state<number | null>(null);

    // ===== Control modes and pages =====

    isDarkroom = $state(false);
    /** Both pager masks are suspended while the strip is in transit. */
    isModeSliding = $state(false);
    #modeSlideTimer: ReturnType<typeof setTimeout> | null = null;

    activePageNormal = $state(1);
    activePageDarkroom = $state(1);
    pagerElNormal = $state<HTMLDivElement | null>(null);
    pagerElDarkroom = $state<HTMLDivElement | null>(null);

    activePage = $derived(this.isDarkroom ? this.activePageDarkroom : this.activePageNormal);
    pageCount = $derived(this.isDarkroom ? 2 : 3);
    pagerEl = $derived<HTMLDivElement | null>(this.isDarkroom ? this.pagerElDarkroom : this.pagerElNormal);

    // The canvases the side bar's histogram draws into, owned by the controls that show them.
    curveHistogramCanvas = $state<HTMLCanvasElement | null>(null);
    masterHistogramCanvas = $state<HTMLCanvasElement | null>(null);
    curveChannel = $state<CurveChannel>("master");

    isColorAnimating = $state(false);
    #colorAnimTimer: ReturnType<typeof setTimeout> | null = null;

    // B&W leaves the colour curves out, the way it pins the colour sliders.
    curveLut = $derived(bakeCurveLut(this.edit.curves, this.edit.isRgb));

    // ===== Crop and zoom =====

    // What the overlay is editing while the tool is open. The preview shows
    // the whole frame for as long as the tool is.
    draftCrop = $state<Crop>({ ...FULL_FRAME });
    isCropping = $state(false);
    isStraightening = $state(false);
    // The crop tool's aspect lock, kept from one image to the next.
    aspectLock = $state<AspectLock>({ ...FREE_ASPECT });
    aspectMenuOpen = $state(false);
    customAspectOpen = $state(false);
    lockedAspect = $derived.by(() =>
        this.imagePayload
            ? aspectRatio(this.aspectLock, this.imagePayload.fullWidth, this.imagePayload.fullHeight)
            : null,
    );

    // Canvas pixels per image pixel, or null while the image is fitted to its
    // slot. The crop tool always shows the image fitted.
    zoom = $state<number | null>(null);
    zoomable = $state(false);

    // The adjustments have nothing to act on without an image, and are set
    // aside while the crop tool has the preview.
    sidebarInactive = $derived(!this.imagePayload || this.isCropping);

    // ===== Histograms =====

    // The darkroom's per-channel histograms follow the crop, so a film border
    // left outside it stops weighing on the black and white points.
    inputHistograms = $state<ChannelHistograms | null>(null);
    #histogramRequest = 0;
    // One binning in flight at a time; requests arriving meanwhile collapse into the latest.
    #histogramBinning = false;
    #histogramPending: HistogramInput | null = null;

    // The strokes both histograms bin. They are out of sight while a brush is
    // in hand, so they catch up once it is put down.
    histogramStrokes = $state.raw<Retouch[]>([]);

    // ===== Export =====

    isExportPending = $state(false);
    showExportModal = $state(false);
    exportMenuOpen = $state(false);
    exportSettings = $state<ExportSettings>({ ...defaultExportSettings });

    // ===== Retouch =====

    // Whether the brushes have every pixel of the image to paint on. The tools
    // open before then; the canvas holds back painting until it is.
    retouchReady = $state(false);
    canRetouch = $derived(!!this.imagePayload && !this.isCropping);
    isRetouching = $derived(retouchTool.mode !== null);

    // ===== Adjustments =====

    // Copying, pasting and resetting act on the sliders of the mode the user
    // is looking at, which includes the ones both modes share.
    activeKeys = $derived(modeKeys(this.isDarkroom));

    isColourMixerIdentity = $derived(HSL_KEYS.every((key) => this.isAtDefault(key)));
    isCurvesIdentity = $derived(curvesEqual(this.edit.curves, defaultCurves()));

    // Whether a reset would change anything, judged on the same keys it writes.
    isIdentity = $derived(
        (this.isDarkroom || this.isCurvesIdentity) &&
            this.activeKeys.every(
                (key) =>
                    ((!this.edit.isRgb || this.isColorAnimating) && key in overridesBW) || this.isAtDefault(key),
            ),
    );

    // ===== Presets =====

    presetDialog = $state<PresetDialog | null>(null);

    // Held by id, so an edited preset previews its new values.
    hoveredPresetId = $state<string | null>(null);
    previewPreset = $derived(
        presets.open && !this.sidebarInactive && !this.presetDialog && this.hoveredPresetId
            ? (presets.list.find((p) => p.id === this.hoveredPresetId) ?? null)
            : null,
    );

    /* What the preview shows: the committed state with the hovered preset laid
       over it, minus what applying it in B&W would divert. */
    shownSliders: Sliders = $derived.by(() => {
        if (!this.previewPreset) return this.edit.sliders;
        const next = { ...this.edit.sliders };
        for (const [key, value] of Object.entries(this.previewPreset.sliders)) {
            if (!this.edit.isRgb && key in overridesBW) continue;
            (next as any)[key] = value;
        }
        return next;
    });

    shownCurveLut = $derived(
        this.previewPreset?.curves
            ? bakeCurveLut({ ...this.edit.curves, ...this.previewPreset.curves }, this.edit.isRgb)
            : this.curveLut,
    );

    // ===== Modes and pages =====

    toggleMode = () => {
        this.isDarkroom = !this.isDarkroom;

        this.isModeSliding = true;
        if (this.#modeSlideTimer) clearTimeout(this.#modeSlideTimer);
        this.#modeSlideTimer = setTimeout(() => {
            this.isModeSliding = false;
            this.#modeSlideTimer = null;
        }, MODE_SLIDE_MS);
    };

    scrollToPage = (index: number) => {
        const el = this.isDarkroom ? this.pagerElDarkroom : this.pagerElNormal;
        el?.scrollTo({ left: index * el.clientWidth, behavior: "smooth" });
    };

    // Jumps without scrolling. A pager that is not mounted yet picks the page
    // up from its active page when it is.
    jumpToPage = (darkroom: boolean, index: number) => {
        if (darkroom) this.activePageDarkroom = index;
        else this.activePageNormal = index;
        const el = darkroom ? this.pagerElDarkroom : this.pagerElNormal;
        if (el && darkroom === this.isDarkroom) {
            el.scrollTo({ left: index * el.clientWidth, behavior: "instant" });
        }
    };

    movePage = (right?: boolean) => {
        const next = right
            ? Math.min(this.activePage + 1, this.pageCount - 1)
            : Math.max(this.activePage - 1, 0);
        this.scrollToPage(next);
    };

    // ===== B&W and invert =====

    #markColorAnimating() {
        this.isColorAnimating = true;
        if (this.#colorAnimTimer) clearTimeout(this.#colorAnimTimer);
        this.#colorAnimTimer = setTimeout(() => {
            this.isColorAnimating = false;
            this.#colorAnimTimer = null;
        }, COLOR_ANIM_MS);
    }

    toggleColorMode = (isBw: boolean) => {
        const edit = this.edit;
        if (isBw) {
            edit.heldColour = {};
            const colorKeys = Object.keys(overridesBW) as (keyof Sliders)[];
            for (const k of colorKeys) {
                (edit.heldColour as any)[k] = edit.sliders[k];
            }
            animateObject(
                edit.sliders as unknown as Record<string, number>,
                overridesBW as Record<string, number>,
                COLOR_ANIM_MS,
            );
            this.#markColorAnimating();
        } else if (edit.heldColour) {
            animateObject(
                edit.sliders as unknown as Record<string, number>,
                edit.heldColour as Record<string, number>,
                COLOR_ANIM_MS,
            );
            this.#markColorAnimating();
            edit.heldColour = null;
        }
    };

    toggleInvert = (isInverted: boolean) => {
        this.commit({ type: "slider", key: "invert", oldValue: !isInverted, newValue: isInverted });
    };

    invertShortcut = () => {
        if (!this.isDarkroom || this.sidebarInactive) return;
        this.edit.sliders.invert = !this.edit.sliders.invert;
        this.toggleInvert(this.edit.sliders.invert);
    };

    // ===== Opening =====

    // A failed open has already cost the backend the image it was holding, so
    // the view is cleared to match rather than left showing pixels that can no
    // longer be exported. A dismissed picker returns null and changes nothing.
    open = async () => {
        try {
            const payload = await openImage();
            if (payload) {
                this.imagePayload = payload;
                this.libraryImageId = null;
                this.zoom = null;
                this.#resetEdit(payload.fullWidth, payload.fullHeight);
                this.#loadMetadata(payload.imageId);
            }
        } catch (e) {
            this.#clearImage();
            console.error("Failed to open image:", e);
        }
    };

    // Opens an image from the library with the edit stored for it, unless that
    // edit was made on a file of another size. Resolves false on failure.
    openLibraryImage = async (
        path: string,
        libraryImageId: number,
        stored: ImageEdit | null,
        storedHistory: EditHistory | null = null,
    ): Promise<boolean> => {
        try {
            const payload = await openImagePath(path);
            this.imagePayload = payload;
            this.zoom = null;
            this.#resetEdit(payload.fullWidth, payload.fullHeight);
            if (stored && editFitsSource(stored, payload.fullWidth, payload.fullHeight)) {
                this.edit = stored;
                if (storedHistory) history.restore(storedHistory.steps, storedHistory.pointer);
            }
            this.libraryImageId = libraryImageId;
            this.#loadMetadata(payload.imageId);
            return true;
        } catch (e) {
            this.#clearImage();
            console.error("Failed to open image:", e);
            return false;
        }
    };

    #clearImage() {
        this.imagePayload = null;
        this.metadata = null;
        this.libraryImageId = null;
    }

    #loadMetadata(imageId: number) {
        this.metadata = null;
        fetchImageMetadata(imageId)
            .then((metadata) => {
                if (this.imagePayload?.imageId === imageId) this.metadata = metadata;
            })
            .catch((e) => console.error("Failed to read image metadata:", e));
    }

    toggleInfo = () => {
        if (!this.infoOpen) {
            presets.open = false;
            retouchTool.mode = null;
        }
        this.infoOpen = !this.infoOpen;
    };

    // An edit belongs to one image, so a newly opened one starts from defaults
    // with an empty history.
    #resetEdit(width: number, height: number) {
        retouchTool.mode = null;
        this.edit = newImageEdit(width, height);
        retouchTool.resetSource();
        this.cancelCrop();
        history.clear();
    }

    toggleZoom = () => {
        this.zoom = this.zoom === null ? 1 : null;
    };

    // ===== Crop =====

    enterCrop = () => {
        presets.open = false;
        retouchTool.mode = null;
        this.zoom = null;
        this.draftCrop = { ...this.edit.crop };
        this.isCropping = true;
        this.#holdAspectOnEntry();
    };

    // An uncropped frame takes on the lock. A crop already drawn to another
    // shape is kept, and the lock lets go.
    #holdAspectOnEntry() {
        if (!this.imagePayload || this.aspectLock.preset === "free") return;
        const { fullWidth, fullHeight } = this.imagePayload;
        const shape = cropAspect(this.draftCrop, fullWidth, fullHeight);
        const uncropped = this.draftCrop.w === 1 && this.draftCrop.h === 1;
        const lock = { ...this.aspectLock, portrait: uncropped ? shape < 1 : this.aspectLock.portrait };
        const ratio = aspectRatio(lock, fullWidth, fullHeight)!;
        if (aspectsMatch(shape, ratio)) this.aspectLock = lock;
        else if (aspectsMatch(shape, 1 / ratio)) this.aspectLock = { ...lock, portrait: !lock.portrait };
        else if (uncropped) this.setAspect(lock);
        else this.aspectLock = { ...FREE_ASPECT };
    }

    // A newly chosen ratio follows the box's current orientation; the draft is
    // then refitted to it.
    setAspect = (lock: AspectLock) => {
        if (!this.imagePayload) return;
        const { fullWidth, fullHeight } = this.imagePayload;
        if (lock.preset !== this.aspectLock.preset && lock.preset !== "custom") {
            lock = { ...lock, portrait: cropAspect(this.draftCrop, fullWidth, fullHeight) < 1 };
        }
        this.aspectLock = lock;
        const ratio = aspectRatio(lock, fullWidth, fullHeight);
        if (ratio === null || aspectsMatch(cropAspect(this.draftCrop, fullWidth, fullHeight), ratio)) return;
        this.draftCrop = cropWithAspect(this.draftCrop, ratio, fullWidth, fullHeight);
    };

    // An entered ratio stands the way it was typed.
    setCustomAspect = (w: number, h: number) => {
        this.setAspect({ preset: "custom", portrait: h > w, custom: [w, h] });
    };

    resetDraft = () => {
        this.draftCrop = { ...FULL_FRAME };
        this.aspectLock = { ...FREE_ASPECT };
    };

    applyCrop = () => {
        this.isCropping = false;
        this.isStraightening = false;
        if (cropsEqual(this.draftCrop, this.edit.crop)) return;
        this.zoom = null;
        this.commit({ type: "crop", oldValue: { ...this.edit.crop }, newValue: { ...this.draftCrop } });
        this.edit.crop = { ...this.draftCrop };
    };

    cancelCrop = () => {
        this.isCropping = false;
        this.isStraightening = false;
    };

    turnDraft = (clockwise: boolean) => {
        this.draftCrop = turnCrop(this.draftCrop, clockwise);
        this.aspectLock = { ...this.aspectLock, portrait: !this.aspectLock.portrait };
    };

    flipDraft = (horizontal: boolean) => {
        this.draftCrop = flipCrop(this.draftCrop, horizontal);
    };

    // Outside the crop tool a flip lands on the applied crop as its own step.
    flipShortcut = (horizontal: boolean) => {
        if (!this.imagePayload) return;
        if (this.isCropping) return this.flipDraft(horizontal);
        const flipped = flipCrop(this.edit.crop, horizontal);
        this.commit({ type: "crop", oldValue: { ...this.edit.crop }, newValue: flipped });
        this.edit.crop = flipped;
    };

    toggleCrop = () => {
        if (!this.imagePayload) return;
        if (this.isCropping) this.applyCrop();
        else this.enterCrop();
    };

    // ===== Histograms =====

    #binCrop(input: HistogramInput) {
        if (this.#histogramBinning) {
            this.#histogramPending = input;
            return;
        }
        this.#histogramBinning = true;
        const request = this.#histogramRequest;
        cropHistograms(input.crop, input.strokes)
            .then((h) => {
                if (request === this.#histogramRequest) this.inputHistograms = h;
            })
            .catch((e) => console.error("Failed to bin the crop:", e))
            .finally(() => {
                this.#histogramBinning = false;
                const next = this.#histogramPending;
                this.#histogramPending = null;
                if (next) this.#binCrop(next);
            });
    }

    // Takes up the applied strokes once no brush is in hand.
    #syncHistogramStrokes() {
        if (this.isRetouching) return;
        const strokes = $state.snapshot(this.edit.retouch) as Retouch[];
        const held = untrack(() => this.histogramStrokes);
        if (strokes.length === held.length && strokes.every((s, i) => retouchEqual(s, held[i]))) return;
        this.histogramStrokes = strokes;
    }

    // Rebins the input histograms for the crop and strokes.
    #updateInputHistograms() {
        const payload = this.imagePayload;
        const kept = { ...this.edit.crop };
        const strokes = this.histogramStrokes;
        ++this.#histogramRequest;
        this.#histogramPending = null;

        if (!payload) {
            this.inputHistograms = null;
        } else if (isFullFrame(kept) && strokes.length === 0) {
            this.inputHistograms = { r: payload.histR, g: payload.histG, b: payload.histB };
        } else {
            this.#binCrop({ crop: kept, strokes });
        }
    }

    // ===== Export =====

    export = async () => {
        this.isExportPending = true;
        // A focused control would otherwise keep taking keys under the overlay.
        (document.activeElement as HTMLElement)?.blur();
        const unlisten = await listen("export:started", () => {
            this.showExportModal = true;
        });
        if (this.isCropping) this.applyCrop();
        try {
            await exportImage(this.edit, this.curveLut, colorSpaceDebug.toneMap, this.exportSettings);
        } finally {
            unlisten();
            this.isExportPending = false;
            this.showExportModal = false;
        }
    };

    // ===== Retouch =====

    toggleRetouch = (mode: RetouchMode) => {
        if (!this.canRetouch) return;
        presets.open = false;
        this.infoOpen = false;
        loupe.picking = false;
        retouchTool.toggle(mode);
    };

    addRetouch = (stroke: Retouch) => {
        this.commit({ type: "retouchAdd", stroke });
        this.edit.retouch = [...this.edit.retouch, stroke];
    };

    clearRetouch = () => {
        if (this.edit.retouch.length === 0) return;
        const strokes = $state.snapshot(this.edit.retouch) as Retouch[];
        this.commit({ type: "retouchClear", strokes });
        this.edit.retouch = [];
    };

    // ===== History =====

    #stateAccessors: StateAccessors = {
        getSlider: (key) => this.edit.sliders[key as keyof Sliders],
        setSlider: (key, val) => {
            (this.edit.sliders as unknown as Record<string, typeof val>)[key] = val;
        },
        // Numbers travel to their new values the way the colour-mode toggle
        // moves them; anything else lands at once.
        setSliders: (values) => {
            const numeric: Record<string, number> = {};
            for (const [key, val] of Object.entries(values)) {
                if (typeof val === "number") numeric[key] = val;
                else (this.edit.sliders as unknown as Record<string, typeof val>)[key] = val;
            }
            animateObject(this.edit.sliders as unknown as Record<string, number>, numeric);
        },
        setCurves: (next) => {
            this.edit.curves = cloneCurves(next);
        },
        setCrop: (value) => {
            this.edit.crop = { ...value };
            this.cancelCrop();
        },
        getRetouch: () => this.edit.retouch,
        setRetouch: (strokes) => {
            this.edit.retouch = [...strokes];
        },
    };

    commit = (action: Action) => {
        history.push(action);
    };

    // Undo or redo inside the crop tool first throws away an unapplied drag;
    // only with nothing to throw away does it reach the history.
    #leaveCropForHistory(): boolean {
        if (!this.isCropping) return false;
        const discarded = !cropsEqual(this.draftCrop, this.edit.crop);
        this.cancelCrop();
        return discarded;
    }

    // Stepping through the history shows the mode and page whose controls it
    // changes. Controls both modes show stay in the mode the user is in.
    #showControlsFor(a: Action) {
        const mode = actionMode(a) ?? (this.isDarkroom ? "darkroom" : "lighter-room");
        const darkroom = mode === "darkroom";
        const page = actionPage(a, mode, darkroom ? this.activePageDarkroom : this.activePageNormal);
        if (page !== null) this.jumpToPage(darkroom, page);
        if (darkroom !== this.isDarkroom) this.toggleMode();
    }

    undo = () => {
        if (this.#leaveCropForHistory()) return;
        const a = history.undo();
        if (!a) return;
        this.#showControlsFor(a);
        undoAction(a, this.#stateAccessors);
    };

    redo = () => {
        if (this.#leaveCropForHistory()) return;
        const a = history.redo();
        if (!a) return;
        this.#showControlsFor(a);
        applyAction(a, this.#stateAccessors);
    };

    // ===== Adjustments =====

    /* In B&W an incoming set puts its colour values aside for the toggle back
       to colour instead of applying them. The set lands as one history step,
       with any curves it carries. */
    #applySliderSet(values: Partial<Sliders>, curveValues: Partial<Curves> = {}) {
        const edit = this.edit;
        const next: Partial<Sliders> = { ...values };

        if (!edit.isRgb) {
            const held: Partial<Sliders> = { ...(edit.heldColour ?? {}) };
            for (const key of Object.keys(overridesBW) as (keyof Sliders)[]) {
                if (key in next) {
                    (held as any)[key] = next[key];
                    delete next[key];
                }
            }
            edit.heldColour = held;
        }

        const oldValues: Partial<Sliders> = {};
        for (const key of Object.keys(next) as (keyof Sliders)[]) {
            (oldValues as any)[key] = edit.sliders[key];
        }

        const oldCurves = cloneCurves($state.snapshot(edit.curves) as Curves);
        const newCurves = cloneCurves({ ...oldCurves, ...curveValues });
        const curvesChange = curvesEqual(oldCurves, newCurves) ? {} : { oldCurves, newCurves };

        this.commit({ type: "sliders", oldValues, newValues: next, ...curvesChange });
        this.#stateAccessors.setSliders(next);
        if (curvesChange.newCurves) this.#stateAccessors.setCurves(curvesChange.newCurves);
    }

    // In B&W the colour keys hold their overrides, so anything that saves the
    // sliders takes the colour values set aside for the toggle back instead.
    #colourSliders(): Sliders {
        if (this.edit.isRgb || !this.edit.heldColour) return this.edit.sliders;
        return { ...this.edit.sliders, ...this.edit.heldColour };
    }

    copyAdjustments = async () => {
        try {
            await copyAdjustments(
                this.#colourSliders(),
                this.activeKeys,
                this.isDarkroom ? undefined : ($state.snapshot(this.edit.curves) as Curves),
            );
        } catch (e) {
            console.error("Failed to copy adjustments:", e);
        }
    };

    // Pasting and resetting change the sliders, so they wait while the
    // sidebar is set aside. Copying only reads them.
    pasteAdjustments = async () => {
        if (this.sidebarInactive) return;
        try {
            const pasted = await readAdjustments(this.activeKeys, !this.isDarkroom);
            this.#applySliderSet(pasted.sliders, pasted.curves);
        } catch (e) {
            console.error("Failed to paste adjustments:", e);
        }
    };

    // Always the colour defaults: in B&W the overridden keys are diverted to
    // the saved colour values, leaving the mode itself untouched.
    #resetKeys(keys: readonly (keyof Sliders)[], curveValues: Partial<Curves> = {}) {
        const next: Partial<Sliders> = {};
        for (const key of keys) {
            (next as any)[key] = defaultSlidersRGB[key];
        }
        this.#applySliderSet(next, curveValues);
    }

    // Lands as one history step and shows the levels page it changed.
    autoLevels = () => {
        if (!this.isDarkroom || this.sidebarInactive || !this.inputHistograms) return;
        const levels = autoLevels(this.inputHistograms);
        if (Object.keys(levels).length === 0) return;
        this.#applySliderSet(levels);
        const page = actionPage({ type: "sliders", oldValues: {}, newValues: levels }, "darkroom", this.activePageDarkroom);
        if (page !== null) this.scrollToPage(page);
    };

    resetAdjustments = () => {
        if (this.sidebarInactive) return;
        this.#resetKeys(this.activeKeys, this.isDarkroom ? {} : defaultCurves());
    };

    resetColourMixer = () => this.#resetKeys(HSL_KEYS);

    /* The tolerance is there because animated values land on their target
       through a lerp, which need not be bit-exact. */
    isAtDefault = (key: keyof Sliders) => {
        const def = defaultSlidersRGB[key];
        const val = this.edit.sliders[key];
        if (typeof def === "number" && typeof val === "number") {
            return Math.abs(val - def) <= SLIDER_TOLERANCE;
        }
        return val === def;
    };

    // ===== Presets =====

    applyPreset = (preset: Preset) => {
        if (this.sidebarInactive) return;
        this.#applySliderSet({ ...preset.sliders }, preset.curves ?? {});
    };

    // A new preset starts from whatever has been changed, or from the whole
    // mode when nothing has.
    openNewPreset = () => {
        let fields = changedFieldIds(this.#colourSliders(), this.edit.curves);
        if (fields.size === 0) fields = modeFieldIds(this.isDarkroom ? "darkroom" : "lighter-room");
        this.presetDialog = { editing: null, name: "", fields };
    };

    openEditPreset = (preset: Preset) => {
        this.presetDialog = { editing: preset, name: preset.name, fields: presetFieldIds(preset) };
    };

    savePresetDialog = (name: string, fields: Set<string>, overwrite: boolean) => {
        const dialog = this.presetDialog;
        this.presetDialog = null;
        if (!dialog) return;

        const current = captureFields(fields, this.#colourSliders(), cloneCurves($state.snapshot(this.edit.curves) as Curves));
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
    };

    exportPreset = async (preset: Preset) => {
        try {
            await presets.exportFile(preset);
        } catch (e) {
            console.error("Failed to export preset:", e);
        }
    };

    importPresets = async () => {
        try {
            const failed = await presets.importFiles();
            if (failed > 0) console.warn(`${failed} preset file(s) could not be read`);
        } catch (e) {
            console.error("Failed to import presets:", e);
        }
    };

    deletePreset = (preset: Preset) => {
        presets.remove(preset.id);
    };

    // The rules that keep the state consistent run for as long as the store
    // exists, whether or not the editor is on screen.
    constructor() {
        // Frame stats start visible in a dev run and hidden in a release build;
        // turning the debug tools on in settings brings F1/F2 back.
        debugStats.enabled = import.meta.env.DEV;

        $effect.root(() => {
            $effect(() => this.#syncHistogramStrokes());
            $effect(() => this.#updateInputHistograms());

            // What a soft proof proofs against is whatever the export is set to write.
            $effect(() => {
                proofing.target = this.exportSettings.outputSpace;
            });

            // An export menu left open when the footer goes inactive would stay up,
            // dimmed and unreachable.
            $effect(() => {
                if (this.sidebarInactive) this.exportMenuOpen = false;
            });

            $effect(() => {
                if (!this.canRetouch) retouchTool.mode = null;
            });

            // The presets need the side bar the brushes send away.
            $effect(() => {
                if (presets.open) untrack(() => (retouchTool.mode = null));
            });

            $effect(() => {
                if (!presets.open) this.hoveredPresetId = null;
            });

            // The presets and the info panel take turns in the same column.
            $effect(() => {
                if (presets.open) untrack(() => (this.infoOpen = false));
            });
        });
    }
}

export const editor = new Editor();
