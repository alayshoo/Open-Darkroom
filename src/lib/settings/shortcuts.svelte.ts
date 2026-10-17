// $lib/settings/shortcuts.svelte.ts

/**
 * Which keys run which action, and the part of that the user has changed.
 *
 * Only the rebound actions are held — everything else falls through to the
 * registry's defaults — so a factory reset is an empty object, and an action
 * whose default changes in a later version follows it unless the user has
 * spoken. The whole map is written as one preference, since a rebind usually
 * touches two actions at once.
 */

import {
    matchesEvent,
    parse,
    shortcutActions,
    display as displayBinding,
    type ShortcutId,
} from "$lib/config/shortcutRegistry";
import { loadSettings, saveSetting } from "./persistence";

const defaultsById = new Map<string, readonly string[]>(
    shortcutActions.map((a) => [a.id, a.defaults]),
);

function sameSet(a: readonly string[], b: readonly string[]) {
    return a.length === b.length && a.every((x) => b.includes(x));
}

/** As much of a stored map as still names real actions and real bindings. */
function sanitise(stored: unknown): Record<string, string[]> {
    if (typeof stored !== "object" || stored === null) return {};

    const clean: Record<string, string[]> = {};
    for (const [id, list] of Object.entries(stored as Record<string, unknown>)) {
        if (!defaultsById.has(id) || !Array.isArray(list)) continue;
        const bindings = list.filter(
            (b): b is string => typeof b === "string" && parse(b) !== null,
        );
        clean[id] = [...new Set(bindings)];
    }
    return clean;
}

class Shortcuts {
    #custom = $state<Record<string, string[]>>({});

    /**
     * Set while a row in the settings tab is listening for a key press. The
     * global handler steps aside for it, so binding Ctrl+Z does not undo.
     */
    recording = $state(false);

    /** Settles once the stored file has been applied; see settings.svelte.ts. */
    readonly hydrated: Promise<void> = loadSettings().then((stored) => {
        if (stored.shortcuts !== undefined) this.#custom = sanitise(stored.shortcuts);
    });

    /** Every binding for an action, custom if there is one and default if not. */
    for(id: ShortcutId): readonly string[] {
        return this.#custom[id] ?? defaultsById.get(id) ?? [];
    }

    /** Whether an action has been changed from what it shipped with. */
    isCustom(id: ShortcutId) {
        return this.#custom[id] !== undefined;
    }

    /** Any action at all changed — what the factory reset button hangs off. */
    get isAnyCustom() {
        return Object.keys(this.#custom).length > 0;
    }

    /** The first binding, formatted for a menu row. Empty if there is none. */
    display(id: ShortcutId) {
        const [first] = this.for(id);
        return first ? displayBinding(first) : "";
    }

    /** Which action a key press runs, if any. */
    actionFor(e: KeyboardEvent): ShortcutId | null {
        for (const action of shortcutActions) {
            for (const binding of this.for(action.id)) {
                if (matchesEvent(binding, e)) return action.id;
            }
        }
        return null;
    }

    /**
     * Gives a binding to an action, taking it off whatever else held it —
     * leaving a key on two actions would make one of them dead. Returns the
     * labels it was taken from, for the row to own up to.
     */
    add(id: ShortcutId, binding: string): string[] {
        const stolen: string[] = [];

        for (const action of shortcutActions) {
            if (action.id === id) continue;
            const held = this.for(action.id);
            if (!held.includes(binding)) continue;
            this.#write(action.id, held.filter((b) => b !== binding));
            stolen.push(action.label);
        }

        const held = this.for(id);
        if (!held.includes(binding)) this.#write(id, [...held, binding]);

        this.#persist();
        return stolen;
    }

    remove(id: ShortcutId, binding: string) {
        this.#write(id, this.for(id).filter((b) => b !== binding));
        this.#persist();
    }

    reset(id: ShortcutId) {
        delete this.#custom[id];
        this.#persist();
    }

    resetAll() {
        this.#custom = {};
        this.#persist();
    }

    /* An action that has been put back the way it shipped stops being an
       override, so the row's revert affordance goes with it. */
    #write(id: string, bindings: string[]) {
        if (sameSet(bindings, defaultsById.get(id) ?? [])) delete this.#custom[id];
        else this.#custom[id] = bindings;
    }

    #persist() {
        void saveSetting("shortcuts", $state.snapshot(this.#custom));
    }
}

export const shortcuts = new Shortcuts();
