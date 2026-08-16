// $lib/settings/settings.svelte.ts

/**
 * App preferences, and the state of the modal that edits them.
 *
 * Every preference the settings modal owns lives here, so a section component
 * stays a view over this store and nothing has to be threaded through the page.
 * Sections themselves are registered in components/settings/sections.ts.
 *
 * Preferences that outlive the session are backed by persistence.ts: each one
 * is a getter over private state whose setter records the new value, so call
 * sites assign as they always did and the file follows.
 */

import { DEFAULTS, loadSettings, saveSetting } from "./persistence";

class Settings {
    /** Whether the modal is up. */
    open = $state(false);

    /** Which section's panel is showing, by its id in the section registry. */
    section = $state("debug");

    /**
     * Master switch for the diagnostic tools — the frame stats overlay and the
     * histogram kill switch. Their shortcuts do nothing while this is off, so a
     * release build ships without them until someone asks for them here.
     */
    #debugTools = $state(DEFAULTS.debugTools);

    get debugTools() {
        return this.#debugTools;
    }

    set debugTools(value: boolean) {
        this.#debugTools = value;
        void saveSetting("debugTools", value);
    }

    /**
     * Settles once the stored file has been applied. Reading is a round trip
     * through the backend, so the defaults are what show until it lands — and
     * the assignments here go to the private fields, since a value that came
     * from the file has no reason to be written back to it.
     */
    readonly hydrated: Promise<void> = loadSettings().then((stored) => {
        if (stored.debugTools !== undefined) this.#debugTools = stored.debugTools;
    });

    show(section?: string) {
        if (section) this.section = section;
        this.open = true;
    }

    close() {
        this.open = false;
    }

    toggle() {
        if (this.open) this.close();
        else this.show();
    }
}

export const settings = new Settings();
