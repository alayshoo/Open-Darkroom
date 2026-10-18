// src/lib/gpu/gamutOverlay.ts
//
// The `GamutOverlay` storage buffer, as chunks/gamutGrids.wgsl declares it.
//
// One description of the gamuts, read by three things: the compute pass that
// counts how much of the image falls outside each one, the diagram that draws
// their triangles, and the panel that lists them. The triangles and the
// percentages therefore cannot disagree — they are the same three points.

import type { GPUSession } from "$lib/types/gpuTypes";
import { OUTPUT_SPACE_IDS, PRIMARIES, type OutputSpaceId } from "./outputSpace";

/**
 * The spectral locus in CIE 1931 xy, at 10nm from 380nm to 700nm. The last
 * point joins the first across the line of purples, which the diagram draws as
 * part of the boundary — a purple is as real a colour as a spectral one, it
 * just takes two wavelengths to make.
 */
export const SPECTRAL_LOCUS: readonly [number, number][] = [
    [0.1741, 0.0050], [0.1738, 0.0049], [0.1733, 0.0048], [0.1726, 0.0048],
    [0.1714, 0.0051], [0.1689, 0.0069], [0.1644, 0.0109], [0.1566, 0.0177],
    [0.1440, 0.0297], [0.1241, 0.0578], [0.0913, 0.1327], [0.0454, 0.2950],
    [0.0082, 0.5384], [0.0139, 0.7502], [0.0743, 0.8338], [0.1547, 0.8059],
    [0.2296, 0.7543], [0.3016, 0.6923], [0.3731, 0.6245], [0.4441, 0.5547],
    [0.5125, 0.4866], [0.5752, 0.4242], [0.6270, 0.3725], [0.6658, 0.3340],
    [0.6915, 0.3083], [0.7079, 0.2920], [0.7190, 0.2809], [0.7260, 0.2740],
    [0.7300, 0.2700], [0.7320, 0.2680], [0.7334, 0.2666], [0.7344, 0.2656],
    [0.7347, 0.2653],
];

/** Must match LOCUS_POINTS in chunks/gamutGrids.wgsl. */
export const LOCUS_POINTS = 33;

/** Must match GAMUT_SPACES there. The order is the order of the slots. */
export const GAMUT_SPACES = 4;

/**
 * Which space each slot in the buffer describes. The export menu's order, so
 * the panel lists them narrowest first.
 */
export const GAMUT_ORDER: readonly OutputSpaceId[] = OUTPUT_SPACE_IDS;

/**
 * The colour each triangle is drawn in. Uploaded with the primaries rather than
 * repeated in the shader, so a swatch in the panel and a line on the diagram
 * are the same value.
 */
export const GAMUT_TINTS: Record<OutputSpaceId, string> = {
    "srgb": "#6fd3ff",
    "display-p3": "#8ae08a",
    "adobe-rgb": "#ffc266",
    "rec2020": "#ff87b3",
};

/** Words per GamutSpace: three chromaticities, the flag and the tint. */
const SPACE_WORDS = 8;
const LOCUS_OFFSET = GAMUT_SPACES * SPACE_WORDS;

export const GAMUT_OVERLAY_BUFFER_SIZE = (LOCUS_OFFSET + LOCUS_POINTS * 2) * 4;

function packTint(hex: string): number {
    return parseInt(hex.slice(1), 16) & 0xffffff;
}

export interface GamutOverlayBuffer {
    buffer: GPUBuffer;
    /** Which triangles the diagram draws, and so which the statistics count. */
    setEnabled: (enabled: Record<OutputSpaceId, boolean>) => void;
    destroy: () => void;
}

export function createGamutOverlayBuffer(gpu: GPUSession): GamutOverlayBuffer {
    const buffer = gpu.device.createBuffer({
        label: "Gamut Overlay Buffer",
        size: GAMUT_OVERLAY_BUFFER_SIZE,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    // One staging block, reused: this is written when a checkbox moves, not per
    // frame, and the locus never changes at all.
    const staging = new ArrayBuffer(GAMUT_OVERLAY_BUFFER_SIZE);
    const floats = new Float32Array(staging);
    const words = new Uint32Array(staging);

    SPECTRAL_LOCUS.forEach(([x, y], i) => {
        floats[LOCUS_OFFSET + i * 2] = x;
        floats[LOCUS_OFFSET + i * 2 + 1] = y;
    });

    GAMUT_ORDER.forEach((id, slot) => {
        const at = slot * SPACE_WORDS;
        PRIMARIES[id].forEach(([x, y], primary) => {
            floats[at + primary * 2] = x;
            floats[at + primary * 2 + 1] = y;
        });
        words[at + 7] = packTint(GAMUT_TINTS[id]);
    });

    function setEnabled(enabled: Record<OutputSpaceId, boolean>) {
        GAMUT_ORDER.forEach((id, slot) => {
            words[slot * SPACE_WORDS + 6] = enabled[id] ? 1 : 0;
        });
        gpu.device.queue.writeBuffer(buffer, 0, staging);
    }

    function destroy() {
        buffer.destroy();
    }

    return { buffer, setEnabled, destroy };
}
