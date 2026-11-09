<!-- routes/+page.svelte -->
<script lang="ts">
    import TitleBar from "$lib/components/window/TitleBar.svelte";
    import Editor from "$lib/components/editor/Editor.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { exportDialog } from "$lib/export/exportDialog.svelte";
</script>

<TitleBar
    undo={editor.undo}
    redo={editor.redo}
    open={editor.open}
    exportImage={exportDialog.show}
    copyAdjustments={editor.copyAdjustments}
    pasteAdjustments={editor.pasteAdjustments}
    resetAdjustments={editor.resetAdjustments}
    openSettings={(section) => settings.show(section)}
    busy={exportDialog.open || exportDialog.busy}
></TitleBar>
<Editor />
