// src/lib/gpu/shaders/colorSpaceEncode.wgsl
//
// Output transform: perceptual working signal → destination colour space.
//
// This is the last stage, and the only one that knows what it is rendering for.
// Four steps: decode the working curve, matrix from the working primaries into
// the destination's, apply the destination transfer curve, clamp.
//
// The working space is linear Rec.2020 and the destination is sRGB, so the
// gamut matrix is a real conversion rather than the identity it collapsed to
// when the two were the same space.
//
// The clamp lives here rather than at the end of develop.wgsl so that the
// headroom above white and the out-of-gamut negatives survive the sharpening
// stage — a negative halo can pull a blown highlight back into range, which it
// could not do if the signal had already been flattened against the ceiling.
//
// The transform itself lives in chunks/outputTransform.wgsl because
// histogram.wgsl has to apply the same one: a histogram of the working space
// would not describe the picture on screen.

//!include "chunks/tone.wgsl"
//!include "chunks/outputTransform.wgsl"

@group(0) @binding(0) var compositeTexture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> @builtin(position) vec4f {
  // Full-screen quad, 2 triangles, 6 vertices.
  var pos = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f( 1.0, -1.0),
    vec2f(-1.0,  1.0),
    vec2f(-1.0,  1.0),
    vec2f( 1.0, -1.0),
    vec2f( 1.0,  1.0),
  );
  return vec4f(pos[vertexIndex], 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let c = textureLoad(compositeTexture, vec2i(position.xy), 0);

  // The working curve is undone exactly: encode_srgb and decode_srgb both pass
  // values outside 0..1 through their linear segment, so the headroom and the
  // negatives reach the matrix intact rather than being flattened on the way.
  let working = decode_srgb(c.rgb);

  return vec4f(to_destination(working), c.a);
}
