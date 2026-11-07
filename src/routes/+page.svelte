<!-- routes/+page.svelte -->
<svelte:options runes />
<script lang="ts">
    import TitleBar from "$lib/components/window/TitleBar.svelte";
    import Editor from "$lib/components/editor/Editor.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import { settings } from "$lib/settings/settings.svelte";
</script>

<TitleBar
    undo={editor.undo}
    redo={editor.redo}
    open={editor.open}
    copyAdjustments={editor.copyAdjustments}
    pasteAdjustments={editor.pasteAdjustments}
    resetAdjustments={editor.resetAdjustments}
    openSettings={(section) => settings.show(section)}
    busy={editor.showExportModal}
></TitleBar>
<Editor />
