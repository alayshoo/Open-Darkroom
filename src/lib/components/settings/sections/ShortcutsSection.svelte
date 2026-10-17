<!-- lib/components/settings/sections/ShortcutsSection.svelte -->
<script lang="ts">
    /*
     * The rebindable actions, grouped the way the registry groups them. Every
     * action can hold any number of bindings; the store keeps only what differs
     * from the defaults, which is what the button at the foot throws away.
     */
    import SettingsGroup from "../SettingsGroup.svelte";
    import ShortcutRow from "../ShortcutRow.svelte";
    import { actionsIn, shortcutGroups } from "$lib/config/shortcutRegistry";
    import { shortcuts } from "$lib/settings/shortcuts.svelte";
</script>

<div class="section flex flex-col gap-4">

    {#each shortcutGroups as group (group.id)}
        <SettingsGroup title={group.label} note={group.note}>
            {#each actionsIn(group.id) as action (action.id)}
                <ShortcutRow {action} />
            {/each}
        </SettingsGroup>
    {/each}

    <div class="footer flex items-center justify-between gap-5">
        <p class="footer-note m-0">
            {shortcuts.isAnyCustom
                ? "Some shortcuts have been changed."
                : "Every shortcut is at its default."}
        </p>
        <button
            class="reset shrink-0 rounded-[8px] border-0 px-3 h-8 cursor-app"
            disabled={!shortcuts.isAnyCustom}
            onclick={() => shortcuts.resetAll()}
        >
            Restore defaults
        </button>
    </div>
</div>

<style>
    .footer {
        padding-left: 12px;
    }

    .footer-note {
        font-family: "Figtree", sans-serif;
        font-size: 11.5px;
        color: var(--color3);
    }

    /* The panel's own row surface, so it belongs to the table above it rather
       than floating over the background. */
    .reset {
        background: var(--bg4);
        font-family: "Figtree", sans-serif;
        font-size: 12.5px;
        font-weight: 500;
        color: var(--color2);
        transition:
            background 0.12s ease,
            color 0.12s ease;
    }

    .reset:hover:not(:disabled) {
        background: var(--bg5);
        color: var(--color1);
    }

    .reset:disabled {
        color: var(--color4);
        cursor: default;
    }
</style>
