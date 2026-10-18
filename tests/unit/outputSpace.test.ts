// tests/unit/outputSpace.test.ts
//
// The frontend derives its destination matrices from published chromaticities,
// the way src-tauri/src/output_space.rs does. `shader_contracts.rs` checks that
// the two sides start from the same numbers; this checks that this side's
// arithmetic turns them into the right matrix.
//
// A transposed or mis-scaled derivation still renders a picture — one with
// crossed channels or a tint — so the properties below are what catch it.

import { describe, expect, it } from "vitest";
import {
    OUTPUT_SPACE_IDS,
    OUTPUT_SPACE_LABELS,
    TRANSFER,
    WORKING_SPACE,
    destinationToWorking,
    transferFor,
    workingToDestination,
    type OutputSpaceId,
} from "$lib/gpu/outputSpace";
import { SRGB_TO_WORKING } from "$lib/gpu/workingSpace";
import { multiplyMat3, type Matrix3x3 } from "$lib/utils/calcMatrixHelpers";

const IDENTITY: Matrix3x3 = [1, 0, 0, 0, 1, 0, 0, 0, 1];

function asMatrix(m: Float32Array): Matrix3x3 {
    return Array.from(m) as Matrix3x3;
}

function expectClose(got: Matrix3x3, want: Matrix3x3, tolerance: number) {
    for (let i = 0; i < 9; i++) {
        expect(got[i], `element ${i} of ${JSON.stringify(got)}`).toBeCloseTo(
            want[i],
            -Math.log10(tolerance),
        );
    }
}

/** Column-major matrix times a column vector. */
function apply(m: Float32Array, v: [number, number, number]): [number, number, number] {
    return [0, 1, 2].map(
        (row) => m[row] * v[0] + m[3 + row] * v[1] + m[6 + row] * v[2],
    ) as [number, number, number];
}

describe("the destination matrices", () => {
    // Rendering to the working space is the one case that converts nothing,
    // which is what makes it the right 16-bit default.
    it("leave the working space alone", () => {
        expectClose(asMatrix(workingToDestination(WORKING_SPACE)), IDENTITY, 1e-5);
    });

    // Soft-proofing goes into a space and straight back out, so an inverse that
    // is not quite an inverse would shift the preview's colour whenever proofing
    // was switched on — which looks exactly like what proofing claims to show.
    it("round trip through their own inverse", () => {
        for (const id of OUTPUT_SPACE_IDS) {
            expectClose(
                multiplyMat3(
                    asMatrix(destinationToWorking(id)),
                    asMatrix(workingToDestination(id)),
                ),
                IDENTITY,
                1e-4,
            );
        }
    });

    // SRGB_TO_WORKING was derived independently, for the input side, and is
    // already under contract against Rust — so it is an outside check that this
    // derivation produces the same conversion in the other direction.
    it("invert the matrix untagged files are read with", () => {
        expectClose(
            multiplyMat3(asMatrix(workingToDestination("srgb")), Array.from(SRGB_TO_WORKING) as Matrix3x3),
            IDENTITY,
            1e-4,
        );
    });

    // Every space here is D65. A drifted white point would tint neutrals and
    // give the shader's gamut compression the wrong grey to aim at.
    it("keep white white", () => {
        for (const id of OUTPUT_SPACE_IDS) {
            const white = apply(workingToDestination(id), [1, 1, 1]);
            for (const channel of white) {
                expect(channel, `${id} white: ${white}`).toBeCloseTo(1, 4);
            }
        }
    });

    // The matrix the shader used to carry as a constant, before the destination
    // became a choice. Pinned as literals because they are what the preview
    // rendered through for every version up to this one.
    it("reproduce the sRGB matrix the shader used to hard-code", () => {
        expectClose(
            asMatrix(workingToDestination("srgb")),
            [
                1.6604910, -0.1245505, -0.0181508,
                -0.5876411, 1.1328999, -0.1005789,
                -0.0728499, -0.0083494, 1.1187297,
            ],
            1e-4,
        );
    });

    // The narrower gamuts sit inside Rec.2020, so their primaries land on
    // working-space values that are positive or barely not. Getting the two
    // spaces the wrong way round instead produces negatives an order of
    // magnitude larger — Rec.2020's own red is about -0.12 in sRGB — which the
    // round trip above cannot see because it is symmetric.
    //
    // The tolerance is not slack: Display P3's red really is a hair outside
    // Rec.2020. Rec.2020's red is the more saturated of the two, but the
    // straight line from it to Rec.2020's green cuts just inside P3's red
    // point, leaving it 0.00125 outside that edge in xy.
    it("place a narrower gamut inside the working space", () => {
        for (const id of OUTPUT_SPACE_IDS.filter((i) => i !== WORKING_SPACE)) {
            const toWorking = destinationToWorking(id);
            for (const primary of [[1, 0, 0], [0, 1, 0], [0, 0, 1]] as [number, number, number][]) {
                for (const channel of apply(toWorking, primary)) {
                    expect(channel, `${id} ${primary}`).toBeGreaterThan(-5e-3);
                }
            }
        }
    });

    // Which is worth pinning on its own, because it is the one place the
    // shader's gamut compression has work to do even on a P3 display showing a
    // P3 export — and because a derivation that quietly clamped it would look
    // like a tidier result rather than a wrong one.
    it("leave Display P3's red marginally outside Rec.2020", () => {
        const red = apply(destinationToWorking("display-p3"), [1, 0, 0]);
        expect(Math.min(...red)).toBeLessThan(0);
        expect(Math.min(...red)).toBeGreaterThan(-5e-3);
    });

    // sRGB and Display P3 differ only in their primaries; Adobe RGB shares
    // sRGB's red and blue. Both are easy to get wrong by copying a row, and
    // neither mistake looks obviously broken on screen.
    it("describe four different spaces", () => {
        for (const a of OUTPUT_SPACE_IDS) {
            for (const b of OUTPUT_SPACE_IDS) {
                if (a === b) continue;
                const difference = asMatrix(workingToDestination(a)).reduce(
                    (sum, v, i) => sum + Math.abs(v - workingToDestination(b)[i]),
                    0,
                );
                expect(difference, `${a} vs ${b}`).toBeGreaterThan(1e-3);
            }
        }
    });
});

describe("the output space table", () => {
    it("gives every space a curve and a label", () => {
        const curves = Object.values(TRANSFER) as number[];
        for (const id of OUTPUT_SPACE_IDS) {
            expect(curves, id).toContain(transferFor(id));
            expect(OUTPUT_SPACE_LABELS[id], id).toBeTruthy();
        }
    });

    // Display P3 is DCI-P3's primaries on sRGB's white and curve — the one pair
    // that shares a transfer function, and the pair most easily conflated.
    it("puts Display P3 on the sRGB curve and Rec.2020 on its own", () => {
        expect(transferFor("srgb")).toBe(TRANSFER.srgb);
        expect(transferFor("display-p3")).toBe(TRANSFER.srgb);
        expect(transferFor("adobe-rgb")).toBe(TRANSFER.adobe);
        expect(transferFor("rec2020")).toBe(TRANSFER.bt2020);
    });

    it("offers the working space, so an export can convert nothing", () => {
        expect(OUTPUT_SPACE_IDS).toContain(WORKING_SPACE);
    });

    // The ids travel to the backend as strings and are matched there by name.
    it("uses the ids the backend deserialises", () => {
        const expected: OutputSpaceId[] = ["srgb", "display-p3", "adobe-rgb", "rec2020"];
        expect([...OUTPUT_SPACE_IDS].sort()).toEqual([...expected].sort());
    });
});
