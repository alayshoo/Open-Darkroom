// Working space ↔ CIE XYZ and Oklab, for the colour-space tools. Checked against
// src/lib/gpu/outputSpace.ts by src-tauri/tests/shader_contracts.rs.

// Linear Rec.2020 → CIE XYZ (D65), column-major. The middle row is LUMA.
const M_WORKING_TO_XYZ = mat3x3<f32>(
    vec3f(0.6369580, 0.2627002, 0.0000000),
    vec3f(0.1446169, 0.6779981, 0.0280727),
    vec3f(0.1688810, 0.0593017, 1.0609851),
);

const M_XYZ_TO_WORKING = mat3x3<f32>(
    vec3f( 1.7166512, -0.6666844,  0.0176399),
    vec3f(-0.3556708,  1.6164812, -0.0427706),
    vec3f(-0.2533663,  0.0157685,  0.9421031),
);

// Oklab, as published: XYZ (D65) → LMS, cube-rooted, → Lab.
const M_XYZ_TO_LMS = mat3x3<f32>(
    vec3f( 0.8189330,  0.0329845,  0.0482003),
    vec3f( 0.3618667,  0.9293119,  0.2643663),
    vec3f(-0.1288597,  0.0361456,  0.6338517),
);

const M_LMS_TO_XYZ = mat3x3<f32>(
    vec3f( 1.2268799, -0.0405757, -0.0763729),
    vec3f(-0.5578150,  1.1122868, -0.4214933),
    vec3f( 0.2813910, -0.0717111,  1.5869240),
);

const M_LMS_TO_LAB = mat3x3<f32>(
    vec3f(0.2104543,  1.9779985,  0.0259040),
    vec3f(0.7936178, -2.4285922,  0.7827718),
    vec3f(-0.0040720, 0.4505937, -0.8086758),
);

const M_LAB_TO_LMS = mat3x3<f32>(
    vec3f(1.0000000,  1.0000000,  1.0000000),
    vec3f(0.3963378, -0.1055613, -0.0894842),
    vec3f(0.2158038, -0.0638542, -1.2914855),
);

// Below this X+Y+Z a pixel has no meaningful chromaticity.
const COLORIMETRY_EPSILON: f32 = 1e-6;

// A signed cube root, since `pow` is undefined for negative bases.
fn cbrt_signed(v: vec3f) -> vec3f {
  return sign(v) * pow(abs(v), vec3f(1.0 / 3.0));
}

fn xyz_to_oklab(xyz: vec3f) -> vec3f {
  return M_LMS_TO_LAB * cbrt_signed(M_XYZ_TO_LMS * xyz);
}

fn oklab_to_working(lab: vec3f) -> vec3f {
  let lms = M_LAB_TO_LMS * lab;
  return M_XYZ_TO_WORKING * (M_LMS_TO_XYZ * (lms * lms * lms));
}
