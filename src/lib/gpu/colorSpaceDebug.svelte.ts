// $lib/gpu/colorSpaceDebug.svelte.ts
//
// Session-only state of the F3 colour-space panel.

import { OUTPUT_SPACE_IDS, type OutputSpaceId } from "./outputSpace";
import { settings } from "$lib/settings/settings.svelte";
import { defaultToneMap, type RolloffMode, type ToneMap } from "$lib/types/toneMap";

/** Triangles shown by default: sRGB and the working space. */
const INITIAL: readonly OutputSpaceId[] = ["srgb", "rec2020"];

function initialOverlays(): Record<OutputSpaceId, boolean> {
    return Object.fromEntries(
        OUTPUT_SPACE_IDS.map((id) => [id, INITIAL.includes(id)]),
    ) as Record<OutputSpaceId, boolean>;
}

class ColorSpaceDebug {
    /** Whether the panel is open; nothing is measured while it is closed. */
    enabled = $state(false);

    overlays = $state<Record<OutputSpaceId, boolean>>(initialOverlays());

    /** Measure after the output transform instead of before it. */
    transformed = $state(false);

    /** Whether the colour volume is binned and drawn. */
    volume = $state(true);

    /** Where the volume is being looked at from, in radians. */
    yaw = $state(0.6);
    pitch = $state(0.42);

    /** Highlight roll-off under test, 0–100. */
    rolloff = $state(defaultToneMap.rolloff);
    rolloffMode = $state<RolloffMode>(defaultToneMap.rolloffMode);

    /**
     * The roll-off the chain and the export use. Only applied while the panel
     * is showing, so closing it is an A/B comparison and nothing leaks out.
     */
    get toneMap(): ToneMap {
        if (!this.enabled || !settings.debugTools) return defaultToneMap;
        return { rolloff: this.rolloff, rolloffMode: this.rolloffMode };
    }

    toggle() {
        this.enabled = !this.enabled;
    }

    toggleOverlay(id: OutputSpaceId) {
        this.overlays = { ...this.overlays, [id]: !this.overlays[id] };
    }

    /** Turn the box by a drag, keeping the pitch off the poles. */
    turn(dx: number, dy: number) {
        this.yaw += dx;
        this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch + dy));
    }
}

export const colorSpaceDebug = new ColorSpaceDebug();
