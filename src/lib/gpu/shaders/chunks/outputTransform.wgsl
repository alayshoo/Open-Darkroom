// The output transform: working space → destination, and the single clamp.
//
// Shared by colorSpaceEncode.wgsl, which renders it, and histogram.wgsl, which
// has to measure the same numbers. A histogram of the working space would
// describe an image nobody is being shown, and would put the clipping warnings
// somewhere other than where the picture clips.
//
// Requires chunks/tone.wgsl for the transfer functions.

// Linear Rec.2020 → linear sRGB (column-major).
const M_WORKING_TO_DESTINATION = mat3x3<f32>(
    vec3f( 1.6604910, -0.1245505, -0.0181508),
    vec3f(-0.5876411,  1.1328999, -0.1005789),
    vec3f(-0.0728499, -0.0083494,  1.1187297),
);

// Linear working space → encoded destination, clamped to the destination gamut.
//
// The clamp is per-channel, so it shifts the hue of whatever it clips: a colour
// inside Rec.2020 but outside sRGB loses its most saturated channel first. A
// gamut map belongs on this line and replaces the clamp without disturbing
// anything else. Clamping after the transfer curve is the same as clamping
// before it — encode_srgb is monotone with encode_srgb(0) = 0 and
// encode_srgb(1) = 1.
fn to_destination(working: vec3f) -> vec3f {
  return clamp(encode_srgb(M_WORKING_TO_DESTINATION * working), vec3f(0.0), vec3f(1.0));
}
