// $lib/settings/persistence.ts

/**
 * Where preferences live between sessions.
 *
 * The store plugin writes under the identifier's app-data directory, which
 * belongs to the OS user — %APPDATA% on Windows, ~/Library/Application Support
 * on macOS, $XDG_DATA_HOME on Linux — so two accounts on one machine keep
 * separate files.
 *
 * Plain TypeScript rather than a rune module: the reading, writing and
 * validating are all here, and settings.svelte.ts is only the reactive view
 * over it.
 */

import { load, type Store } from "@tauri-apps/plugin-store";

/** Sits next to the identifier's other per-user data. */
export const SETTINGS_FILE = "settings.json";

/**
 * The preferences that outlive a session, each with the value used when the
 * file says nothing about it. A new persisted setting is a line here.
 */
export const DEFAULTS = {
    debugTools: import.meta.env.DEV,
};

export type Persisted = typeof DEFAULTS;
export type PersistedKey = keyof Persisted;

/** One store for the process. Opened lazily so importing this costs nothing. */
let opening: Promise<Store> | null = null;

function store() {
    opening ??= load(SETTINGS_FILE);
    return opening;
}

/**
 * Outside a Tauri window — `vite dev` in a plain browser, or the browser test
 * run — there is no IPC to carry the call. Preferences fall back to their
 * defaults rather than taking the app down with them.
 */
function unavailable(error: unknown) {
    console.warn(`Settings could not reach ${SETTINGS_FILE}:`, error);
    opening = null;
}

/**
 * The stored value for every key whose recorded type still matches its default.
 * A hand-edited file is read as far as it makes sense and no further, so one
 * bad line costs one preference rather than all of them.
 */
export async function loadSettings(): Promise<Partial<Persisted>> {
    let entries: [string, unknown][];
    try {
        entries = await (await store()).entries();
    } catch (error) {
        unavailable(error);
        return {};
    }

    const settings: Record<string, unknown> = {};
    for (const [key, value] of entries) {
        if (!(key in DEFAULTS)) continue;
        if (typeof value !== typeof DEFAULTS[key as PersistedKey]) continue;
        settings[key] = value;
    }
    return settings as Partial<Persisted>;
}

/**
 * Records one preference, flushed rather than left pending — a toggle is
 * usually the last thing that happens before the window closes. Failing to
 * write is not worth interrupting an edit over.
 */
export async function saveSetting<K extends PersistedKey>(
    key: K,
    value: Persisted[K],
): Promise<void> {
    try {
        const file = await store();
        await file.set(key, value);
        await file.save();
    } catch (error) {
        unavailable(error);
    }
}
