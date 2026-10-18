// lib/utils/calcMatrixHelpers.ts


export type Matrix3x3  = [
    number, number, number,
    number, number, number,
    number, number, number
];

export type Matrix4x4 = [
    number, number, number, number,
    number, number, number, number,
    number, number, number, number,
    number, number, number, number,
];

export function multiplyMat4(a: Matrix4x4, b: Matrix4x4): Matrix4x4 {
    const result = new Array(16) as Matrix4x4;
    for (let col = 0; col < 4; col++) {
      for (let row = 0; row < 4; row++) {
        result[col * 4 + row] =
          a[row]      * b[col * 4]     +
          a[4 + row]  * b[col * 4 + 1] +
          a[8 + row]  * b[col * 4 + 2] +
          a[12 + row] * b[col * 4 + 3];
      }
    }
    return result;
  }
// Column-major throughout, matching what a WGSL mat3x3 expects: element
// [col * 3 + row].

export function multiplyMat3(a: Matrix3x3, b: Matrix3x3): Matrix3x3 {
    const result = new Array(9) as Matrix3x3;
    for (let col = 0; col < 3; col++) {
        for (let row = 0; row < 3; row++) {
            result[col * 3 + row] =
                a[row]     * b[col * 3]     +
                a[3 + row] * b[col * 3 + 1] +
                a[6 + row] * b[col * 3 + 2];
        }
    }
    return result;
}

// Inverse by cofactors. Callers build their matrices from three independent
// colour primaries, so the determinant cannot vanish.
export function invertMat3(m: Matrix3x3): Matrix3x3 {
    const at = (col: number, row: number) => m[col * 3 + row];

    const cofactor = (col: number, row: number) => {
        const c = [(col + 1) % 3, (col + 2) % 3];
        const r = [(row + 1) % 3, (row + 2) % 3];
        return at(c[0], r[0]) * at(c[1], r[1]) - at(c[1], r[0]) * at(c[0], r[1]);
    };

    const det =
        at(0, 0) * cofactor(0, 0) +
        at(1, 0) * cofactor(1, 0) +
        at(2, 0) * cofactor(2, 0);

    // The cofactor of (col, row) belongs at (row, col) in the inverse.
    const result = new Array(9) as Matrix3x3;
    for (let col = 0; col < 3; col++) {
        for (let row = 0; row < 3; row++) {
            result[row * 3 + col] = cofactor(col, row) / det;
        }
    }
    return result;
}

export function multiplyMat3Vec(m: Matrix3x3, v: [number, number, number]): [number, number, number] {
    return [0, 1, 2].map(
        (row) => m[row] * v[0] + m[3 + row] * v[1] + m[6 + row] * v[2],
    ) as [number, number, number];
}
