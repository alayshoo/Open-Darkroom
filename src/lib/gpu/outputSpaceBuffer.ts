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
    transferFor,
    workingToDestination,
    type OutputSpaceId,
} from "./outputSpace";

/**
 * Three `mat3x3<f32>` of three 16-byte columns each, then two u32, padded to the
 * 16 bytes a uniform struct binds at. Must match `OUTPUT_SPACE_BYTES` in
 * src-tauri/src/export_rendering.rs.
 */
export const OUTPUT_SPACE_BUFFER_SIZE = 160;

/** Where each field starts, in units of the view that writes it. */
const TO_DESTINATION = 0;
const TO_PROOF = 12;
const FROM_PROOF = 24;
const CURVE_WORD = 36;
const PROOF_WORD = 37;

export interface OutputSpaceBuffer {
    buffer: GPUBuffer;
    /**
     * Point the transform at a destination, optionally proofing against the
     * space an export would be written in on the way.
     */
    set: (destination: OutputSpaceId, proof: OutputSpaceId | null) => void;
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

    function set(destination: OutputSpaceId, proof: OutputSpaceId | null) {
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

        gpu.device.queue.writeBuffer(buffer, 0, staging);
    }

    function destroy() {
        buffer.destroy();
    }

    return { buffer, set, destroy };
}
