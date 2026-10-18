// How the chain measures a pixel's brightness and its saturation.
//
// BT.2020 luminance coefficients, matching the Rec.2020 working space the
// adjustment chain runs in. Must stay equal to LR/LG/LB in calcParams.wgsl:
// the saturation matrix is built there from those, and vibrance mixes towards
// the grey computed from these.
const LUMA = vec3f(0.2627002, 0.6779981, 0.0593017);

// Linear Rec.2020 → linear sRGB (column-major), the fixed reference vibrance
// judges how saturated a pixel already looks against.
//
// Deliberately not the output transform's matrix. Vibrance is a develop-stage
// decision, so the same sliders have to produce the same image whatever space
// the export is being written in — a measurement that followed the destination
// would quietly re-grade the picture when the export format changed.
const M_WORKING_TO_SRGB = mat3x3<f32>(
    vec3f( 1.6604910, -0.1245505, -0.0181508),
    vec3f(-0.5876411,  1.1328999, -0.1005789),
    vec3f(-0.0728499, -0.0083494,  1.1187297),
);
