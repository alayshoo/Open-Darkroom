// $lib/gpu/colorSpaceDebug.svelte.ts
//
// Session-only state of the F3 colour-space panel.

import { OUTPUT_SPACE_IDS, type OutputSpaceId } from "./outputSpace";

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
