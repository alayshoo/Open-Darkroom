// $lib/config/keyboardShortcuts.ts

/**
 * The window's keydown handler. It owns none of the bindings — it asks the
 * shortcut store which action a press names and calls the matching handler, so
 * a rebind in settings takes effect without anything here changing.
 */

import type { ShortcutId } from "./shortcutRegistry";
import { shortcuts } from "$lib/settings/shortcuts.svelte";

interface ShortcutDeps {
    /** One entry per action in the registry. */
    handlers: Record<ShortcutId, () => void>;
    closeSettings: () => void;
    isSettingsOpen: () => boolean;
}

export function isTextField(target: EventTarget | null) {
    return (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
    );
}

/**
 * A field with the caret in it owns the ordinary meaning of what is typed:
 * plain characters, and the clipboard pair whatever they are bound to.
 */
function standsAsideFor(id: ShortcutId, e: KeyboardEvent) {
    if (!isTextField(e.target)) return false;
    if (id === "copyAdjustments" || id === "pasteAdjustments") return true;
    return e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey;
}

export function createKeydownHandler({ handlers, closeSettings, isSettingsOpen }: ShortcutDeps) {
    return (e: KeyboardEvent) => {
        // A row in the shortcuts tab is waiting for this press, not us.
        if (shortcuts.recording) return;

        const id = shortcuts.actionFor(e);

        // The settings modal takes the keyboard while it is up: only what
        // dismisses it gets through to the editing shortcuts.
        if (isSettingsOpen()) {
            if (e.key === "Escape" || id === "toggleSettings") {
                closeSettings();
                e.preventDefault();
            }
            return;
        }

        if (!id || standsAsideFor(id, e)) return;

        handlers[id]();
        e.preventDefault();
        (document.activeElement as HTMLElement)?.blur();
    };
}
