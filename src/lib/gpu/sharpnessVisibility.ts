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

// Must match TEXTURE_SIGMA_HI, CLARITY_SIGMA_FRACTION, MAX_CLARITY_SIGMA and
// MIN_SIGMA in calcParams.wgsl, and MAX_RADIUS in calcSharpTexture.wgsl.
const TEXTURE_SIGMA_HI = 5.0;
const CLARITY_SIGMA_FRACTION = 0.01;
const MAX_CLARITY_SIGMA = 60.0;
const MIN_SIGMA = 0.7;
const MAX_RADIUS = 180;

/**
 * How many render pixels the blurs read on either side of a pixel, or 0 when
 * the sharpness stage is skipped. Mirrors `sharp_margin_rows` in
 * export_rendering.rs, brought down to the render scale, and errs wide.
 */
export function sharpnessReach(
    sliders: Sliders,
    renderScale: number,
    fullWidth: number,
    fullHeight: number,
): number {
    if (!sharpnessIsVisible(sliders, renderScale)) return 0;
    let sigma = 0;
    if (sliders.usmAmount !== 0) sigma = Math.max(sigma, sliders.usmRadius);
    if (sliders.texture !== 0) sigma = Math.max(sigma, TEXTURE_SIGMA_HI);
    if (sliders.clarity !== 0) {
        const clarity = Math.min(fullWidth, fullHeight) * CLARITY_SIGMA_FRACTION;
        sigma = Math.max(sigma, Math.min(Math.max(clarity, MIN_CLARITY_SIGMA), MAX_CLARITY_SIGMA));
    }
    return Math.min(MAX_RADIUS, Math.ceil(3 * Math.max(sigma * renderScale, 2 * MIN_SIGMA)));
}
