// src/lib/gpu/shaders/colorSpaceEncode.wgsl
//
// Output transform: perceptual working signal → destination colour space.
//
// This is the last stage, and the only one that knows what it is rendering for.
// The full form is decode the working curve, matrix into the destination
// primaries, apply the destination transfer curve, clamp. Working space and
// destination are both sRGB today, so the first three collapse: the gamut
// matrix is the identity, and encode_srgb is monotone with encode_srgb(0) = 0
// and encode_srgb(1) = 1, so clamping the encoded signal is exactly clamping
// the linear one before encoding it. Only the clamp is left.
//
// The clamp lives here rather than at the end of develop.wgsl so that the
// headroom above white and the out-of-gamut negatives survive the sharpening
// stage — a negative halo can pull a blown highlight back into range, which it
// could not do if the signal had already been flattened against the ceiling.

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
  return vec4f(clamp(c.rgb, vec3f(0.0), vec3f(1.0)), c.a);
}
