// tests/unit/calcMatrixHelpers.test.ts
//
// The matrix helpers feed the WGSL uniforms, which are column-major. Getting
// the convention wrong transposes the transform — visible as crossed colour
// channels rather than an obvious error.

import { describe, expect, it } from "vitest";
import { multiplyMat4, type Matrix4x4 } from "$lib/utils/calcMatrixHelpers";

const IDENTITY: Matrix4x4 = [
  1, 0, 0, 0,
  0, 1, 0, 0,
  0, 0, 1, 0,
  0, 0, 0, 1,
];

/** Column-major scale matrix. */
function scale(s: number): Matrix4x4 {
  return [s, 0, 0, 0, 0, s, 0, 0, 0, 0, s, 0, 0, 0, 0, 1];
}

/** Column-major translation: the offset lives in the last column. */
function translate(x: number, y: number, z: number): Matrix4x4 {
  return [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1];
}

describe("multiplyMat4", () => {
  it("leaves a matrix unchanged when multiplied by the identity", () => {
    const m: Matrix4x4 = [
      1, 2, 3, 4,
      5, 6, 7, 8,
      9, 10, 11, 12,
      13, 14, 15, 16,
    ];

    expect(multiplyMat4(IDENTITY, m)).toEqual(m);
    expect(multiplyMat4(m, IDENTITY)).toEqual(m);
  });

  it("multiplies diagonal matrices element-wise", () => {
    const a: Matrix4x4 = [2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 0, 0, 0, 1];
    const b: Matrix4x4 = [5, 0, 0, 0, 0, 6, 0, 0, 0, 0, 7, 0, 0, 0, 0, 1];

    expect(multiplyMat4(a, b)).toEqual([
      10, 0, 0, 0,
      0, 18, 0, 0,
      0, 0, 28, 0,
      0, 0, 0, 1,
    ]);
  });

  it("applies the right-hand matrix first, in column-major order", () => {
    // scale ∘ translate scales the offset; translate ∘ scale does not. If the
    // convention were transposed these two would come out swapped.
    const scaleThenTranslate = multiplyMat4(scale(2), translate(5, 6, 7));
    expect(scaleThenTranslate.slice(12)).toEqual([10, 12, 14, 1]);

    const translateThenScale = multiplyMat4(translate(5, 6, 7), scale(2));
    expect(translateThenScale.slice(12)).toEqual([5, 6, 7, 1]);
  });

  it("is not commutative", () => {
    const a = scale(2);
    const b = translate(1, 2, 3);
    expect(multiplyMat4(a, b)).not.toEqual(multiplyMat4(b, a));
  });

  it("returns sixteen finite numbers", () => {
    const result = multiplyMat4(scale(3), translate(1, 1, 1));
    expect(result).toHaveLength(16);
    expect(result.every(Number.isFinite)).toBe(true);
  });
});
