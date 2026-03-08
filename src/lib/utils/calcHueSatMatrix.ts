// src/lib/utils/calcHueSatMatrix.ts

import { type Matrix4x4, multiplyMat4 } from "$lib/utils/calcMatrixHelpers";

export function calcHueSatMatrix(saturation: number, hue: number): Matrix4x4 {
    const lr = 0.2126, lg = 0.7152, lb = 0.0722;
  
    // Saturation matrix: lerp toward luminance
    const sr = (1 - saturation) * lr;
    const sg = (1 - saturation) * lg;
    const sb = (1 - saturation) * lb;
  
    // Saturation only (column-major)
    const satMat: Matrix4x4 = [
      sr + saturation, sr,              sr,              0,
      sg,              sg + saturation, sg,              0,
      sb,              sb,              sb + saturation, 0,
      0,               0,               0,               1,
    ];
  
    if (hue === 0) return satMat;
  
    // Hue rotation around (1,1,1) axis
    const cos = Math.cos(hue);
    const sin = Math.sin(hue);
    const k = 1 / 3; // (1-cos)/3
    const sq = Math.sqrt(1 / 3);
  
    const hueMat: Matrix4x4 = [
      cos + (1 - cos) / 3,
      (1 - cos) / 3 - sq * sin,
      (1 - cos) / 3 + sq * sin,
      0,
      (1 - cos) / 3 + sq * sin,
      cos + (1 - cos) / 3,
      (1 - cos) / 3 - sq * sin,
      0,
      (1 - cos) / 3 - sq * sin,
      (1 - cos) / 3 + sq * sin,
      cos + (1 - cos) / 3,
      0,
      0, 0, 0, 1,
    ];
  
    // Compose: hue × sat (you likely have a mat4 multiply helper)
    return multiplyMat4(hueMat, satMat);
  }