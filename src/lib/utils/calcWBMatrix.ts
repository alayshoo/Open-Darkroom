// src/lib/utils/wbMatrixCalc.ts

// --- Matrix helpers (row-major internally) ---

type Mat3 = [
    number, number, number,
    number, number, number,
    number, number, number
];

function matMul(A: Mat3, B: Mat3): Mat3 {
    return [
        A[0]*B[0] + A[1]*B[3] + A[2]*B[6],
        A[0]*B[1] + A[1]*B[4] + A[2]*B[7],
        A[0]*B[2] + A[1]*B[5] + A[2]*B[8],

        A[3]*B[0] + A[4]*B[3] + A[5]*B[6],
        A[3]*B[1] + A[4]*B[4] + A[5]*B[7],
        A[3]*B[2] + A[4]*B[5] + A[5]*B[8],

        A[6]*B[0] + A[7]*B[3] + A[8]*B[6],
        A[6]*B[1] + A[7]*B[4] + A[8]*B[7],
        A[6]*B[2] + A[7]*B[5] + A[8]*B[8],
    ];
}

function mulVec3(M: Mat3, v: [number, number, number]): [number, number, number] {
    return [
        M[0]*v[0] + M[1]*v[1] + M[2]*v[2],
        M[3]*v[0] + M[4]*v[1] + M[5]*v[2],
        M[6]*v[0] + M[7]*v[1] + M[8]*v[2],
    ];
}

function diagMat(s: [number, number, number]): Mat3 {
    return [
        s[0], 0,    0,
        0,    s[1], 0,
        0,    0,    s[2],
    ];
}

// Row-major -> column-major Float32Array for WGSL mat3x3f
function toWGSL(M: Mat3): Float32Array<ArrayBuffer> {
    const buf = new Float32Array(new ArrayBuffer(48)); // 12 floats * 4 bytes
    buf.set([
        M[0], M[3], M[6], 0,
        M[1], M[4], M[7], 0,
        M[2], M[5], M[8], 0,
    ]);
    return buf;
}

// --- Constants ---

const M_BRADFORD: Mat3 = [
     0.8951,  0.2664, -0.1614,
    -0.7502,  1.7135,  0.0367,
     0.0389, -0.0685,  1.0296,
];

const M_BRADFORD_INV: Mat3 = [
     0.9869929, -0.1470543,  0.1599627,
     0.4323053,  0.5183603,  0.0492912,
    -0.0085287,  0.0400428,  0.9684867,
];

// Linear sRGB <-> XYZ D65
const M_RGB_TO_XYZ: Mat3 = [
    0.4124564, 0.3575761, 0.1804375,
    0.2126729, 0.7151522, 0.0721750,
    0.0193339, 0.1191920, 0.9503041,
];

const M_XYZ_TO_RGB: Mat3 = [
     3.2406, -1.5372, -0.4986,
    -0.9689,  1.8758,  0.0415,
     0.0557, -0.2040,  1.0570,
];

// --- CCT -> XYZ (Planckian locus, Kim et al.) ---

function cctToXYZ(temp: number): [number, number, number] {
    const T = Math.max(1000, Math.min(25000, temp));

    let x: number;
    if (T <= 4000) {
        x =
            -0.2661239e9 / T ** 3 -
            0.234358e6 / T ** 2 +
            0.8776956e3 / T +
            0.17991;
    } else {
        x =
            -3.0258469e9 / T ** 3 +
            2.1070379e6 / T ** 2 +
            0.2226347e3 / T +
            0.24039;
    }

    let y: number;
    if (T <= 4000) {
        y =
            -1.1063814 * x ** 3 -
            1.3481102 * x ** 2 +
            2.1855583 * x -
            0.2021968;
    } else {
        y =
            3.081758 * x ** 3 -
            5.8733867 * x ** 2 +
            3.75113 * x -
            0.3700148;
    }

    return [x / y, 1.0, (1 - x - y) / y];
}

// --- Bradford CAT + tint -> 3x3 RGB matrix ---

const NEUTRAL_XYZ = cctToXYZ(5500); // computed once

export function temperatureTintToWBMatrix(
    temp: number,
    tint: number
): Float32Array<ArrayBuffer> {
    const srcXYZ = cctToXYZ(temp); // "actual" illuminant (slider)
    const dstXYZ = NEUTRAL_XYZ;    // neutral reference (5500K)

    // Map both illuminants into Bradford cone space
    const srcCone = mulVec3(M_BRADFORD, srcXYZ);
    const dstCone = mulVec3(M_BRADFORD, dstXYZ);

    // Diagonal scale: adapt src -> dst
    const scale: [number, number, number] = [
        dstCone[0] / srcCone[0],
        dstCone[1] / srcCone[1],
        dstCone[2] / srcCone[2],
    ];

    // Full CAT in XYZ space
    const M_CAT_XYZ = matMul(
        M_BRADFORD_INV,
        matMul(diagMat(scale), M_BRADFORD)
    );

    // Bring into linear sRGB space
    const M_CAT_RGB = matMul(M_XYZ_TO_RGB, matMul(M_CAT_XYZ, M_RGB_TO_XYZ));

    // Tint: perpendicular to Planckian locus (magenta/green axis)
    // Applied as a green channel scale post-CAT
    // ±100 tint ≈ ±1 stop on green
    const tintScale = Math.pow(2, -tint / 100);
    const M_TINT: Mat3 = diagMat([1, tintScale, 1]);

    return toWGSL(matMul(M_TINT, M_CAT_RGB));
}