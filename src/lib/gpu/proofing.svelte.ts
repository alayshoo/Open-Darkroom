// $lib/gpu/proofing.svelte.ts
//
// Whether the preview is being shown through the gamut of the space an export
// would be written in.
//
// A wide-gamut display shows colour an sRGB file cannot hold, so the preview and
// the file disagree exactly where it matters most — saturated film reds and
// skies. Turning this on squeezes the preview through the export space and back,
// which costs the preview its own gamut but makes it honest about the result.
// The histogram follows, so the clipping warnings move with it.

import type { OutputSpaceId } from "./outputSpace";
import type { GamutMask } from "./outputSpaceBuffer";
import { GAMUT_TINTS, packTint } from "./gamutOverlay";

class Proofing {
    /** Whether the preview is being squeezed through the export space. */
    enabled = $state(false);

    /** The export space to proof against, mirrored from the export settings. */
    target = $state<OutputSpaceId>("srgb");

    /** What the render chain should be told, which is nothing while this is off. */
    get space(): OutputSpaceId | null {
        return this.enabled ? this.target : null;
    }

    /** Whether the pixels the export space cannot hold are painted over the preview. */
    warning = $state(false);

    /** Measured before the output transform, which would otherwise fit everything in. */
    get mask(): GamutMask | null {
        if (!this.warning) return null;
        return {
            space: this.target,
            tint: packTint(GAMUT_TINTS[this.target]),
            afterTransform: false,
        };
    }

    toggle() {
        this.enabled = !this.enabled;
    }

    toggleWarning() {
        this.warning = !this.warning;
    }
}

export const proofing = new Proofing();
