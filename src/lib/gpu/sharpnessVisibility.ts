// src/lib/gpu/sharpnessVisibility.ts

import type { Sliders } from "$lib/types/imgParameters";

// Must match SIGMA_FADE_START in calcParams.wgsl.
export const SIGMA_FADE_START = 0.3;

// Must match TEXTURE_SIGMA_LO and MIN_CLARITY_SIGMA in calcParams.wgsl. Clarity
// uses its floor rather than its actual radius: the real one depends on the
// image dimensions, and a floor only ever over-estimates visibility, which is
// the safe direction to be wrong in — see below.
const TEXTURE_SIGMA_LO = 1.2;
const MIN_CLARITY_SIGMA = 20.0;

/**
 * Whether the sharpness stage can change a single pixel of this render.
 *
 * This duplicates the front half of the fade in `calcParams.wgsl`, and the
 * duplication is unavoidable: skipping a render pass is a decision the CPU has
 * to make while recording the frame, and the values it depends on live in a
 * storage buffer that only the GPU has written. Reading it back would stall the
 * pipeline every frame to save two passes — far worse than the passes cost.
 *
 * Only the predicate is duplicated, not the maths. If this returns true when the
 * shader would have faded every band to nothing, the frame is merely slower, not
 * wrong: `calcParams.wgsl` zeroes the amounts over the same range, so composite
 * is a no-op regardless of what the detail texture holds. That asymmetry is
 * deliberate — this is an optimisation, and the shader remains the authority on
 * the result.
 */
export function sharpnessIsVisible(sliders: Sliders, renderScale: number): boolean {
    const resolvable = (sigmaFullRes: number) =>
        sigmaFullRes * renderScale > SIGMA_FADE_START;

    if (sliders.clarity !== 0 && resolvable(MIN_CLARITY_SIGMA)) return true;
    if (sliders.texture !== 0 && resolvable(TEXTURE_SIGMA_LO)) return true;
    if (sliders.usmAmount !== 0 && resolvable(sliders.usmRadius)) return true;
    return false;
}
