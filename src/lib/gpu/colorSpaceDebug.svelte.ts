// $lib/gpu/colorSpaceDebug.svelte.ts
//
// Whether the colour-space panel is up, and which gamuts it is drawing.
//
// Kept beside the frame stats rather than in settings: this is a diagnostic
// whose state is only meaningful while the panel is open, so there is nothing
// worth outliving the session. The panel renders when something it depends on
// changes — a slider, a new image, a checkbox — and not otherwise.

import { OUTPUT_SPACE_IDS, type OutputSpaceId } from "./outputSpace";

/** Which triangles the diagram starts with: the one everything is compared
 *  against, and the working space the chain runs in, which is the ceiling
 *  nothing in the image can plot outside of. */
const INITIAL: readonly OutputSpaceId[] = ["srgb", "rec2020"];

function initialOverlays(): Record<OutputSpaceId, boolean> {
    return Object.fromEntries(
        OUTPUT_SPACE_IDS.map((id) => [id, INITIAL.includes(id)]),
    ) as Record<OutputSpaceId, boolean>;
}

class ColorSpaceDebug {
    /** Whether the sidebar is showing, and so whether anything is measured at
     *  all — a full pass over the overview is not free. */
    enabled = $state(false);

    overlays = $state<Record<OutputSpaceId, boolean>>(initialOverlays());

    /**
     * Whether the panel measures what the output transform produces rather than
     * what the adjustments asked for.
     *
     * Off is the question the plots exist to answer — the colour the edit wants,
     * which the transform has not yet had to give up. On is what the screen is
     * showing, and the difference between the two is the gamut compression
     * doing its work. The percentages read differently either side of it: after
     * the transform, nothing can fall outside the space being rendered to, so a
     * zero there says the compression worked rather than that the colour fitted.
     */
    transformed = $state(false);

    /**
     * Whether the colour volume is drawn at all.
     *
     * It is the expensive half of the panel — a ray per fragment, and a third
     * grid to fill — and it is the half that is not always wanted, so it
     * switches off on its own. The grid is not even binned while it is off.
     */
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

    /** Turn the box by a drag, keeping the eye off the poles where the
     *  lightness axis would collapse to a point. */
    turn(dx: number, dy: number) {
        this.yaw += dx;
        this.pitch = Math.max(-1.45, Math.min(1.45, this.pitch + dy));
    }
}

export const colorSpaceDebug = new ColorSpaceDebug();
