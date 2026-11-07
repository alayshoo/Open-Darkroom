<!-- lib/components/editor/Editor.svelte -->
<script lang="ts">
    import PreviewImageCanvas from "$lib/components/outputs/PreviewImageCanvas.svelte";
    import CropOverlay from "$lib/components/outputs/CropOverlay.svelte";
    import DebugStatsOverlay from "$lib/components/outputs/DebugStatsOverlay.svelte";
    import ColorSpacePanel from "$lib/components/outputs/ColorSpacePanel.svelte";
    import RetouchBar from "$lib/components/retouch/RetouchBar.svelte";
    import PresetsPanel from "$lib/components/presets/PresetsPanel.svelte";
    import PresetModal from "$lib/components/presets/PresetModal.svelte";
    import ExportModal from "$lib/components/ExportModal.svelte";
    import CustomAspectModal from "$lib/components/CustomAspectModal.svelte";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { loupe } from "$lib/gpu/loupe.svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { presets } from "$lib/presets/presets.svelte";
    import { retouchTool } from "$lib/retouch/retouchTool.svelte";
    import { holdPageZoom } from "$lib/utils/pageZoomGuard";
    import { createKeydownHandler, isTextField } from "$lib/config/keyboardShortcuts";
    import type { ShortcutId } from "$lib/config/shortcutRegistry";
    import { editor } from "$lib/editor/editor.svelte";
    import EditorToolbar from "./EditorToolbar.svelte";
    import EditorSideBar from "./EditorSideBar.svelte";

    $effect(() => {
        document.documentElement.dataset.theme = editor.isDarkroom ? "darkroom" : "";
    });

    $effect(() => holdPageZoom());

    // A brush in hand keeps the keyboard to itself, bar undo and redo.
    const RETOUCH_SHORTCUTS: ReadonlySet<ShortcutId> = new Set(["healTool", "cloneTool", "undo", "redo"]);

    const handleKeydown = createKeydownHandler({
        handlers: {
            pagePrev: () => editor.movePage(false),
            pageNext: () => editor.movePage(true),
            undo: editor.undo,
            redo: editor.redo,
            changeMode: editor.toggleMode,
            openImage: editor.open,
            copyAdjustments: editor.copyAdjustments,
            pasteAdjustments: editor.pasteAdjustments,
            resetAdjustments: editor.resetAdjustments,
            toggleCrop: editor.toggleCrop,
            healTool: () => editor.toggleRetouch("heal"),
            cloneTool: () => editor.toggleRetouch("clone"),
            flipHorizontal: () => editor.flipShortcut(true),
            flipVertical: () => editor.flipShortcut(false),
            autoLevels: editor.autoLevels,
            toggleInvert: editor.invertShortcut,
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
        allows: (id) => !editor.isRetouching || RETOUCH_SHORTCUTS.has(id),
    });

    // Enter and Escape belong to the crop tool while it is open, unless a field
    // or the export menu has the keyboard.
    function handleWindowKeydown(e: KeyboardEvent) {
        // Nothing but the window itself answers while an export runs.
        if (editor.isExportPending) {
            e.preventDefault();
            return;
        }
        // The preset dialog takes the keyboard while it is up.
        if (editor.presetDialog) {
            if (e.key === "Escape") {
                editor.presetDialog = null;
                e.preventDefault();
            }
            return;
        }
        // So does the custom ratio dialog, which answers its own keys.
        if (editor.customAspectOpen) return;
        if (loupe.picking && e.key === "Escape" && !settings.open && !isTextField(e.target)) {
            loupe.picking = false;
            e.preventDefault();
            return;
        }
        if (retouchTool.mode && e.key === "Escape" && !settings.open && !isTextField(e.target)) {
            retouchTool.mode = null;
            e.preventDefault();
            return;
        }
        const ownsKey =
            editor.isCropping &&
            !settings.open &&
            !editor.exportMenuOpen &&
            !isTextField(e.target);
        if (ownsKey && (e.key === "Enter" || e.key === "Escape")) {
            if (e.key === "Enter") editor.applyCrop();
            else if (editor.aspectMenuOpen) editor.aspectMenuOpen = false;
            else if (editor.isStraightening) editor.isStraightening = false;
            else editor.cancelCrop();
            e.preventDefault();
            (document.activeElement as HTMLElement)?.blur();
            return;
        }
        handleKeydown(e);
    }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<!-- Full-window canvas background. The image is centred on .canvas-slot but is
     free to spill out of it in every direction once zoomed; the toolbar and
     side bar sit on a higher layer and simply paint over the overflow. -->
<div class="canvas-backdrop"></div>
<div class="app-shell flex absolute flex-row gap-3">
    <div class="chrome-scrim" class:shifted={presets.open}>
        <div class="scrim-fill"></div>
    </div>
    <RetouchBar open={editor.isRetouching} ready={editor.retouchReady} strokes={editor.edit.retouch.length} onclear={editor.clearRetouch} />
    <div class="presets-column shrink-0" class:open={presets.open} inert={!presets.open}>
        <div class="presets-slide h-full">
            <PresetsPanel
                list={presets.list}
                canApply={!editor.sidebarInactive}
                onapply={editor.applyPreset}
                onhover={(preset) => (editor.hoveredPresetId = preset?.id ?? null)}
                onadd={editor.openNewPreset}
                onimport={editor.importPresets}
                onedit={editor.openEditPreset}
                onexport={editor.exportPreset}
                ondelete={editor.deletePreset}
            />
        </div>
    </div>
    <EditorToolbar />
    <!-- Its own column, so opening it shifts the image rather than covering it. -->
    {#if settings.debugTools && colorSpaceDebug.enabled}
        <div class="colour-space-bar shrink-0">
            <ColorSpacePanel sliders={editor.shownSliders} curveLut={editor.shownCurveLut} imagePayload={editor.imagePayload} />
        </div>
    {/if}
    <div class="canvas-slot relative flex-1">
        <div class="canvas-center absolute flex items-center justify-center">
            <PreviewImageCanvas
                sliders={editor.shownSliders}
                curveLut={editor.shownCurveLut}
                imagePayload={editor.imagePayload}
                crop={editor.isCropping ? editor.draftCrop : editor.edit.crop}
                showWhole={editor.isCropping}
                retouch={editor.edit.retouch}
                onretouch={editor.addRetouch}
                bind:retouchReady={editor.retouchReady}
                bind:zoom={editor.zoom}
                bind:zoomable={editor.zoomable}
            >
                {#snippet overlay()}
                    {#if editor.isCropping && editor.imagePayload}
                        <CropOverlay
                            bind:crop={editor.draftCrop}
                            bind:straightening={editor.isStraightening}
                            aspect={editor.lockedAspect}
                            imageWidth={editor.imagePayload.fullWidth}
                            imageHeight={editor.imagePayload.fullHeight}
                        />
                    {/if}
                {/snippet}
            </PreviewImageCanvas>
        </div>
        {#if settings.debugTools && debugStats.enabled}
            <DebugStatsOverlay />
        {/if}
    </div>
    <EditorSideBar />
</div>

<ExportModal visible={editor.showExportModal}></ExportModal>
<CustomAspectModal
    bind:open={editor.customAspectOpen}
    initial={editor.aspectLock.custom ?? [1, 1]}
    onapply={editor.setCustomAspect}
/>

{#if editor.presetDialog}
    <PresetModal
        editing={editor.presetDialog.editing !== null}
        initialName={editor.presetDialog.name}
        initialFields={editor.presetDialog.fields}
        onsave={editor.savePresetDialog}
        oncancel={() => (editor.presetDialog = null)}
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
        /* Smoothstep, from a scrim band's inner edge out to the window edge. */
        --scrim-ramp: transparent, rgba(0, 0, 0, 0.06) 15%, rgba(0, 0, 0, 0.22) 30%,
            rgba(0, 0, 0, 0.42) 45%, rgba(0, 0, 0, 0.65) 60%,
            rgba(0, 0, 0, 0.84) 75%, rgba(0, 0, 0, 0.97) 90%, black;
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

    /* The gradient under the toolbar and title bar, turning the corner on a
       rounded inner edge. A dense band and a wide faint tail are stacked, both
       on an eased ramp so neither shows where it starts. */
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
            linear-gradient(to top, var(--scrim-ramp)) var(--reach-x) 0 /
                calc(100% - var(--reach-x)) var(--top) no-repeat,
            linear-gradient(to left, var(--scrim-ramp)) 0 var(--reach-y) /
                var(--left) calc(100% - var(--reach-y)) no-repeat;
        transition: opacity 0.5s ease;
    }

    /* Each band's depth, and where the corner tile hands over to it. The
       hand-overs stay on multiples of 4px so the seams land on whole device
       pixels at common display scales. */
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
</style>
