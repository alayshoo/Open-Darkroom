// The colorimetry the colour-space tools measure in: the working space against
// CIE XYZ, and Oklab for the lightness and hue axes.
//
// The working space is fixed at Rec.2020, so these are constants rather than
// uniforms — the same reason M_WORKING_TO_SRGB in chunks/luma.wgsl is one.
// src-tauri/tests/shader_contracts.rs checks them against what
// src/lib/gpu/outputSpace.ts derives from the published chromaticities.

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

// Oklab, as published: XYZ (D65) → a cone response, cube-rooted, → Lab.
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

// Below this the pixel carries no light worth asking the hue of, and the
// division that finds its chromaticity would not be meaningful either.
const COLORIMETRY_EPSILON: f32 = 1e-6;

// CIE 1931 xy of a colour, or a negative y for one too dark to have any.
fn chromaticity(working: vec3f) -> vec2f {
  let xyz = M_WORKING_TO_XYZ * max(working, vec3f(0.0));
  let sum = xyz.x + xyz.y + xyz.z;
  if (sum < COLORIMETRY_EPSILON) { return vec2f(0.0, -1.0); }
  return vec2f(xyz.x, xyz.y) / sum;
}

// A signed cube root: the chain carries out-of-gamut negatives, and `pow` on a
// negative number is undefined.
fn cbrt_signed(v: vec3f) -> vec3f {
  return sign(v) * pow(abs(v), vec3f(1.0 / 3.0));
}

fn working_to_oklab(working: vec3f) -> vec3f {
  return M_LMS_TO_LAB * cbrt_signed(M_XYZ_TO_LMS * (M_WORKING_TO_XYZ * working));
}

fn oklab_to_working(lab: vec3f) -> vec3f {
  let lms = M_LAB_TO_LMS * lab;
  return M_XYZ_TO_WORKING * (M_LMS_TO_XYZ * (lms * lms * lms));
}

// Lightness, chroma and hue in turns, which is what the volume plot bins on.
fn oklab_to_lch(lab: vec3f) -> vec3f {
  let hue = atan2(lab.z, lab.y) / (2.0 * 3.14159265);
  return vec3f(lab.x, length(vec2f(lab.y, lab.z)), fract(hue + 1.0));
}

fn lch_to_oklab(lch: vec3f) -> vec3f {
  let angle = lch.z * 2.0 * 3.14159265;
  return vec3f(lch.x, lch.y * cos(angle), lch.y * sin(angle));
}
