/* $lib/utils/autoLevels.ts */

import type { Sliders } from "$lib/types/imgParameters";
import type { ChannelHistograms } from "./cropHistograms";

/** Share of the pixels left outside the points at each end, per channel. */
export const AUTO_LEVELS_CLIP = 0.001;

/** Narrowest span the two points are allowed to close to, in slider units. */
const MIN_SPAN = 1;

const SLIDER_MAX = 255;

// Bin i holds sRGB values from i/256 up to (i+1)/256, while the sliders read
// sRGB on a 0–255 scale.
const binToSlider = (bin: number) => (bin / 256) * SLIDER_MAX;

/**
 * Where the running count first reaches `target`, as a fractional bin,
 * spreading each bin's pixels evenly across its width.
 */
function binAtCount(hist: ArrayLike<number>, target: number): number {
    let seen = 0;
    for (let i = 0; i < hist.length; i++) {
        const count = hist[i];
        if (count > 0 && seen + count >= target) return i + (target - seen) / count;
        seen += count;
    }
    return hist.length;
}

/** One channel's black and white point, or null for an empty histogram. */
export function channelLevels(
    hist: ArrayLike<number>,
    clip = AUTO_LEVELS_CLIP,
): { black: number; white: number } | null {
    let total = 0;
    for (let i = 0; i < hist.length; i++) total += hist[i];
    if (total === 0) return null;

    let black = binToSlider(binAtCount(hist, total * clip));
    let white = binToSlider(binAtCount(hist, total * (1 - clip)));

    if (white - black < MIN_SPAN) {
        const mid = Math.min(Math.max((black + white) / 2, MIN_SPAN / 2), SLIDER_MAX - MIN_SPAN / 2);
        black = mid - MIN_SPAN / 2;
        white = mid + MIN_SPAN / 2;
    }
    return { black, white };
}

/**
 * Per-channel black and white points that span what each channel holds, so a
 * negative's compressed, offset histograms each fill the range.
 */
export function autoLevels(hists: ChannelHistograms, clip = AUTO_LEVELS_CLIP): Partial<Sliders> {
    const levels: Partial<Sliders> = {};
    const channels = [
        [hists.r, "redBlackPoint", "redWhitePoint"],
        [hists.g, "greenBlackPoint", "greenWhitePoint"],
        [hists.b, "blueBlackPoint", "blueWhitePoint"],
    ] as const;

    for (const [hist, blackKey, whiteKey] of channels) {
        const points = channelLevels(hist, clip);
        if (!points) continue;
        levels[blackKey] = points.black;
        levels[whiteKey] = points.white;
    }
    return levels;
}
