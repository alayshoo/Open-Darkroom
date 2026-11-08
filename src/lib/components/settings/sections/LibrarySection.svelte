<!-- lib/components/settings/sections/LibrarySection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import SettingsRow from "../SettingsRow.svelte";
    import SettingsToggle from "../SettingsToggle.svelte";

    import { settings } from "$lib/settings/settings.svelte";
    import { library } from "$lib/library/library.svelte";
    import { choosePhotoFolder } from "$lib/library/catalogApi";

    async function choose() {
        try {
            const folder = await choosePhotoFolder();
            if (folder) library.photoFolder = folder;
        } catch (e) {
            console.error("Failed to choose the photo folder:", e);
        }
    }
</script>

<div class="section flex flex-col gap-4">
    <SettingsGroup>
        <SettingsRow label="Photo folder" description={library.photoFolder ?? "Not chosen yet"}>
            <button class="choose" onclick={choose}>{library.photoFolder ? "Change…" : "Choose…"}</button>
        </SettingsRow>
        <SettingsRow label="Copy imports into the photo folder">
            <SettingsToggle
                checked={settings.importCopies}
                label="Copy imports into the photo folder"
                onclick={() => (settings.importCopies = !settings.importCopies)}
            />
        </SettingsRow>
    </SettingsGroup>

    <SettingsGroup title="Layout">
        <SettingsRow label="Group rolls in a library">
            <SettingsToggle
                checked={settings.groupRolls}
                label="Group rolls in a library"
                onclick={() => (settings.groupRolls = !settings.groupRolls)}
            />
        </SettingsRow>
    </SettingsGroup>
</div>

<style>
    .choose {
        height: 28px;
        padding: 0 12px;
        border: 0;
        border-radius: 8px;
        background: var(--bg5);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        white-space: nowrap;
        cursor: default;
    }

    .choose:hover {
        background: var(--color5);
    }
</style>
