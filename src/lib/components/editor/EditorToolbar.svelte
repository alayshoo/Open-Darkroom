<!-- lib/components/editor/EditorToolbar.svelte -->
<script lang="ts">
    import CropButton from "$lib/components/inputs/cropButton.svelte";
    import CropTools from "$lib/components/inputs/cropTools.svelte";
    import RetouchTools from "$lib/components/inputs/retouchTools.svelte";
    import PresetsButton from "$lib/components/inputs/presetsButton.svelte";
    import ZoomButton from "$lib/components/inputs/zoomButton.svelte";
    import InfoButton from "$lib/components/inputs/infoButton.svelte";
    import { loupe } from "$lib/gpu/loupe.svelte";
    import { presets } from "$lib/presets/presets.svelte";
    import { isFullFrame } from "$lib/types/crop";
    import { editor } from "$lib/editor/editor.svelte";
</script>

<!-- The empty parts of the icon column drag the window; Tauri matches the
     attribute on the event target alone, so the buttons are unaffected. -->
<div data-tauri-drag-region class="toolbar shrink-0">
    <div
        data-tauri-drag-region
        class="toolbar-tools relative flex flex-col items-center"
        class:dimmed={!editor.imagePayload}
        inert={!editor.imagePayload}
    >
        <div class="crop-group relative">
            <CropButton active={editor.isCropping} ontoggle={editor.toggleCrop} />
            <CropTools
                visible={editor.isCropping}
                bind:straightening={editor.isStraightening}
                bind:aspectMenuOpen={editor.aspectMenuOpen}
                aspect={editor.aspectLock}
                aspectRatio={editor.lockedAspect}
                canReset={!isFullFrame(editor.draftCrop)}
                onturn={editor.turnDraft}
                onflip={editor.flipDraft}
                onaspect={editor.setAspect}
                oncustom={() => (editor.customAspectOpen = true)}
                onreset={editor.resetDraft}
            />
        </div>
        <div class="toolbar-rest flex flex-col items-center gap-1" class:away={editor.isCropping} inert={editor.isCropping}>
            <RetouchTools disabled={editor.isCropping} ontoggle={editor.toggleRetouch} />
            <PresetsButton
                active={presets.open}
                disabled={editor.isCropping}
                ontoggle={() => presets.toggle()}
            />
        </div>
    </div>
    <div
        data-tauri-drag-region
        class="toolbar-zoom absolute flex flex-col items-center gap-1"
        class:dimmed={!editor.imagePayload}
        inert={!editor.imagePayload}
    >
        <InfoButton active={editor.infoOpen} ontoggle={editor.toggleInfo} />
        <ZoomButton
            fitted={editor.zoom === null}
            disabled={editor.isCropping || loupe.picking || (editor.zoom === null && !editor.zoomable)}
            ontoggle={editor.toggleZoom}
        />
    </div>
</div>

<style>
    /* The icon column. It bleeds past the shell's insets to the window's left,
       top and bottom edges so its buttons sit on the scrim's left band. */
    .toolbar {
        position: relative;
        margin: -44px 0 -12px -12px;
        min-width: 48px;
        z-index: 10;
        pointer-events: auto;
    }

    .toolbar-tools {
        transition: opacity 0.2s ease;
        padding-top: 50px;
        padding-right: 6px;
        gap: 4px;
        z-index: 1;
    }

    /* Fades out of the crop tools' way, and back once they have folded up. */
    .toolbar-rest {
        transition: opacity 0.18s ease 0.18s;
    }

    .toolbar-rest.away {
        opacity: 0;
        transition-delay: 0s;
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
</style>
