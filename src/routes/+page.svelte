<!-- routes/+page.svelte -->
<script lang="ts">
    import { onMount } from "svelte";
    import TitleBar from "$lib/components/window/TitleBar.svelte";
    import Editor from "$lib/components/editor/Editor.svelte";
    import Library from "$lib/components/library/Library.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import { library } from "$lib/library/library.svelte";
    import { settings } from "$lib/settings/settings.svelte";

    let editing = $derived(library.showing === "editor");
    const inEditor = (action: () => void) => () => {
        if (editing) action();
    };

    onMount(() => {
        void library.load();
    });
</script>

<TitleBar
    undo={inEditor(editor.undo)}
    redo={inEditor(editor.redo)}
    open={library.openFile}
    importFiles={library.importFiles}
    importFolder={library.importFolder}
    toggleLibrary={library.toggleView}
    {editing}
    copyAdjustments={inEditor(editor.copyAdjustments)}
    pasteAdjustments={inEditor(editor.pasteAdjustments)}
    resetAdjustments={inEditor(editor.resetAdjustments)}
    openSettings={(section) => settings.show(section)}
    busy={editor.showExportModal}
></TitleBar>
<Library active={!editing} />
<!-- Kept mounted while the library is up, so going back to the image is instant. -->
<div class="editor-layer" class:away={!editing}>
    <Editor active={editing} canvasHidden={library.canvasHidden || (!editing && !library.leaving)} />
</div>

<style>
    /* The window's size, so the editor lays out as it would on its own. Clicks
       fall through to the library wherever the editor does not take them. */
    .editor-layer {
        position: fixed;
        inset: 0;
        z-index: 1;
        pointer-events: none;
    }

    /* Out of sight once the side bars have slid off and the image has flown. */
    .editor-layer.away {
        visibility: hidden;
        transition: visibility 0s linear 0.9s;
    }
</style>
