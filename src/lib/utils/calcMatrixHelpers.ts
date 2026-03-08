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