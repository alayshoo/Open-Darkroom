// lib/utils/calcDarkroomMatrix.ts

import { type Matrix4x4 } from "$lib/utils/calcMatrixHelpers"


export function calcDarkroomMatrix(
    invert: boolean,
    redBlackPoint: number,
    greenBlackPoint: number,
    blueBlackPoint: number,
    redWhitePoint: number,
    greenWhitePoint: number,
    blueWhitePoint: number,
    rgbOutputBlack: number,
    rgbOutputWhite: number,
): Matrix4x4 {
    const outRange = rgbOutputWhite - rgbOutputBlack;

    const outStart = invert ? rgbOutputWhite : rgbOutputBlack;
    const outDir = invert ? -outRange : outRange;

    const sR = outDir / (redWhitePoint - redBlackPoint);
    const sG = outDir / (greenWhitePoint - greenBlackPoint);
    const sB = outDir / (blueWhitePoint - blueBlackPoint);

    const oR = -redBlackPoint * sR + outStart;
    const oG = -greenBlackPoint * sG + outStart;
    const oB = -blueBlackPoint * sB + outStart;

    // column-major to match WGSL/WebGPU convention
    return [
        sR, 0,  0,  0,
        0,  sG, 0,  0,
        0,  0,  sB, 0,
        oR, oG, oB, 1,
    ];
}