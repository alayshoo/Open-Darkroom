// BT.2020 luminance coefficients, matching the Rec.2020 working space the
// adjustment chain runs in. Must stay equal to LR/LG/LB in calcParams.wgsl:
// the saturation matrix is built there from those, and vibrance mixes towards
// the grey computed from these.
const LUMA = vec3f(0.2627002, 0.6779981, 0.0593017);
