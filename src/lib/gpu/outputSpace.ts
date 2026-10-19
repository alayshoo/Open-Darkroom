// src/lib/gpu/outputSpace.ts
//
// The spaces a render can be sent to: the display it is previewed on, and the
// one an export is written in.
//
// The chromaticities below are the only colour numbers duplicated between the
// frontend and the backend — src-tauri/src/output_space.rs states the same ones,
// and shader_contracts.rs checks that they still agree. Everything else is
// derived from them on both sides, so a space cannot be described one way to the
// preview and another way to the file.

import {
    invertMat3,
    multiplyMat3,
    multiplyMat3Vec,
    type Matrix3x3,
} from "$lib/utils/calcMatrixHelpers";

export type OutputSpaceId = "srgb" | "display-p3" | "adobe-rgb" | "rec2020";

/**
 * Transfer curve ids. Must match `Transfer` in src-tauri/src/output_space.rs
 * and TRANSFER_* in shaders/chunks/outputTransform.wgsl.
 */
export const TRANSFER = {
    srgb: 0,
    adobe: 1,
    bt2020: 2,
} as const;

/** Every space, in the order the export menu offers them. */
export const OUTPUT_SPACE_IDS: OutputSpaceId[] = [
    "srgb",
    "display-p3",
    "adobe-rgb",
    "rec2020",
];

/**
 * The working space the whole render chain runs in. Choosing it as a
 * destination is the one case where no gamut conversion happens at all.
 */
export const WORKING_SPACE: OutputSpaceId = "rec2020";

/** Shared by every space here, which is what lets the gamut compression in
 *  outputTransform.wgsl treat one luminance as the neutral axis of all of them. */
export const D65: [number, number] = [0.3127, 0.3290];

/** Published chromaticities of the red, green and blue primaries. */
export const PRIMARIES: Record<OutputSpaceId, [number, number][]> = {
    "srgb":       [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060]],
    "display-p3": [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]],
    "adobe-rgb":  [[0.640, 0.330], [0.210, 0.710], [0.150, 0.060]],
    "rec2020":    [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]],
};

const TRANSFERS: Record<OutputSpaceId, number> = {
    // Display P3 is DCI-P3's primaries with sRGB's white and transfer curve.
    "srgb": TRANSFER.srgb,
    "display-p3": TRANSFER.srgb,
    "adobe-rgb": TRANSFER.adobe,
    "rec2020": TRANSFER.bt2020,
};

/** What the picker calls each space. */
export const OUTPUT_SPACE_LABELS: Record<OutputSpaceId, string> = {
    "srgb": "sRGB",
    "display-p3": "Display P3",
    "adobe-rgb": "Adobe RGB",
    "rec2020": "Rec.2020",
};

export function transferFor(id: OutputSpaceId): number {
    return TRANSFERS[id];
}

/**
 * Linear RGB → XYZ for a set of primaries and a white point.
 *
 * Each primary contributes a column, scaled so that RGB (1, 1, 1) lands exactly
 * on the white point — the reason the matrix cannot simply be read off the
 * chromaticities.
 */
function rgbToXyz(primaries: [number, number][], white: [number, number]): Matrix3x3 {
    const m = new Array(9).fill(0) as Matrix3x3;
    primaries.forEach(([x, y], col) => {
        m[col * 3] = x / y;
        m[col * 3 + 1] = 1;
        m[col * 3 + 2] = (1 - x - y) / y;
    });

    const w: [number, number, number] = [
        white[0] / white[1],
        1,
        (1 - white[0] - white[1]) / white[1],
    ];
    const scale = multiplyMat3Vec(invertMat3(m), w);

    for (let col = 0; col < 3; col++) {
        for (let row = 0; row < 3; row++) {
            m[col * 3 + row] *= scale[col];
        }
    }
    return m;
}

/**
 * Linear working space → linear destination, column-major for WGSL.
 *
 * Both spaces are D65, so this is the destination's primaries undone and the
 * working space's applied, with no chromatic adaptation in between.
 */
export function workingToDestination(id: OutputSpaceId): Float32Array {
    const working = rgbToXyz(PRIMARIES[WORKING_SPACE], D65);
    const destination = rgbToXyz(PRIMARIES[id], D65);
    return new Float32Array(multiplyMat3(invertMat3(destination), working));
}

/**
 * Linear destination → linear working space. Soft-proofing needs the round
 * trip: into the space being proofed, and back out to carry on to the display.
 */
export function destinationToWorking(id: OutputSpaceId): Float32Array {
    const working = rgbToXyz(PRIMARIES[WORKING_SPACE], D65);
    const destination = rgbToXyz(PRIMARIES[id], D65);
    return new Float32Array(multiplyMat3(invertMat3(working), destination));
}

/** Linear RGB of a space → CIE XYZ, column-major. */
export function spaceToXyz(id: OutputSpaceId): Float32Array {
    return new Float32Array(rgbToXyz(PRIMARIES[id], D65));
}

/** The working space's, which the shaders hold as M_WORKING_TO_XYZ. */
export function workingToXyz(): Float32Array {
    return spaceToXyz(WORKING_SPACE);
}
