// src/lib/gpu/outputSpaceBuffer.ts
//
// The `OutputSpace` uniform, as chunks/outputTransform.wgsl declares it.
//
// The preview and the histogram each keep their own: they run separate copies
// of the chain and have to be told the same thing, the same way the source
// primaries are already handed to both.

import type { GPUSession } from "$lib/types/gpuTypes";
import {
    destinationToWorking,
    PRIMARIES,
    spaceToSpace,
    spaceToXyz,
    transferFor,
    workingToDestination,
    type OutputSpaceId,
} from "./outputSpace";

/**
 * Three `mat3x3<f32>` of three 16-byte columns each, four u32, then the mask's
 * `mat3x3<f32>` and `vec3f`, padded to 16. Must match `OUTPUT_SPACE_BYTES` in
 * src-tauri/src/export_rendering.rs.
 */
export const OUTPUT_SPACE_BUFFER_SIZE = 224;

/** Where each field starts, in units of the view that writes it. */
const TO_DESTINATION = 0;
const TO_PROOF = 12;
const FROM_PROOF = 24;
const CURVE_WORD = 36;
const PROOF_WORD = 37;
const MASK_WORD = 38;
const MASK_TINT_WORD = 39;
const TO_MASK = 40;
const MASK_HEIGHTS = 52;

/** MASK_ ids in chunks/outputTransform.wgsl. */
const MASK_BEFORE = 1;
const MASK_AFTER = 2;

/** The pixels of the preview painted as outside a gamut. */
export interface GamutMask {
    space: OutputSpaceId;
    /** 0xRRGGBB. */
    tint: number;
    /** Test what the output transform produces rather than what it was given. */
    afterTransform: boolean;
}

/** Each primary's X+Y+Z, and the triangle's height in xy above the edge opposite it. */
function triangleTerms(space: OutputSpaceId) {
    const xyz = spaceToXyz(space);
    const sums = [0, 1, 2].map((i) => xyz[i * 3] + xyz[i * 3 + 1] + xyz[i * 3 + 2]);
    const p = PRIMARIES[space];
    const heights = p.map(([x, y], i) => {
        const [ax, ay] = p[(i + 1) % 3];
        const [bx, by] = p[(i + 2) % 3];
        const cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
        return Math.abs(cross) / Math.hypot(bx - ax, by - ay);
    });
    return { sums, heights };
}

export interface OutputSpaceBuffer {
    buffer: GPUBuffer;
    /**
     * Point the transform at a destination, optionally proofing against the
     * space an export would be written in on the way.
     */
    set: (destination: OutputSpaceId, proof: OutputSpaceId | null) => void;
    /** Paint the pixels outside a gamut, or stop with null. Preview only. */
    setMask: (mask: GamutMask | null) => void;
    destroy: () => void;
}

export function createOutputSpaceBuffer(gpu: GPUSession): OutputSpaceBuffer {
    const buffer = gpu.device.createBuffer({
        label: "Output Space Uniform Buffer",
        size: OUTPUT_SPACE_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    // One staging block, reused: this is written when a setting changes, not
    // per frame, but there is no reason to allocate for it either.
    const staging = new ArrayBuffer(OUTPUT_SPACE_BUFFER_SIZE);
    const floats = new Float32Array(staging);
    const words = new Uint32Array(staging);

    // A WGSL mat3x3<f32> is three 16-byte columns, not nine packed floats, so
    // each column goes into its own vec4 slot. Packed tightly, the second column
    // lands four bytes early and every colour comes out skewed.
    function writeMatrix(at: number, m: Float32Array) {
        for (let col = 0; col < 3; col++) {
            floats.set(m.subarray(col * 3, col * 3 + 3), at + col * 4);
        }
    }

    let destination: OutputSpaceId = gpu.displaySpace;
    let proof: OutputSpaceId | null = null;
    let mask: GamutMask | null = null;

    function write() {
        floats.fill(0);
        writeMatrix(TO_DESTINATION, workingToDestination(destination));

        // Left at zero when nothing is being proofed: the shader reads neither
        // matrix unless the flag below is set.
        if (proof) {
            writeMatrix(TO_PROOF, workingToDestination(proof));
            writeMatrix(FROM_PROOF, destinationToWorking(proof));
        }

        words[CURVE_WORD] = transferFor(destination);
        words[PROOF_WORD] = proof ? 1 : 0;

        if (mask) {
            words[MASK_WORD] = mask.afterTransform ? MASK_AFTER : MASK_BEFORE;
            words[MASK_TINT_WORD] = mask.tint & 0xffffff;

            // Rows weighted by each primary's X+Y+Z, so the shader can find the
            // chromaticity's distance from each edge as inside_gamut() does.
            const toMask = mask.afterTransform
                ? spaceToSpace(destination, mask.space)
                : workingToDestination(mask.space);
            const { sums, heights } = triangleTerms(mask.space);
            for (let col = 0; col < 3; col++) {
                for (let row = 0; row < 3; row++) toMask[col * 3 + row] *= sums[row];
            }
            writeMatrix(TO_MASK, toMask);
            floats.set(heights, MASK_HEIGHTS);
        }

        gpu.device.queue.writeBuffer(buffer, 0, staging);
    }

    function set(to: OutputSpaceId, proofAgainst: OutputSpaceId | null) {
        destination = to;
        proof = proofAgainst;
        write();
    }

    function setMask(next: GamutMask | null) {
        mask = next;
        write();
    }

    function destroy() {
        buffer.destroy();
    }

    return { buffer, set, setMask, destroy };
}
