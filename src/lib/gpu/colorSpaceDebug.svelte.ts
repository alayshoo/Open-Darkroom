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

    toggle() {
        this.enabled = !this.enabled;
    }

    toggleOverlay(id: OutputSpaceId) {
        this.overlays = { ...this.overlays, [id]: !this.overlays[id] };
    }
}

export const colorSpaceDebug = new ColorSpaceDebug();
