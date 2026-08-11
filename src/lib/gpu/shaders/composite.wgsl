// src/lib/gpu/shaders/composite.wgsl
//
// Combines the developed image with the sharpness detail bands produced by
// calcSharpTexture.wgsl. Sits between develop.wgsl and colorSpaceEncode.wgsl and
// operates on the perceptual working signal, which is where sharpening belongs:
// a fixed halo amplitude in linear light would mean something different at every
// brightness, and applying it after the output transform would tie the edit to
// the destination colour space.
//
// The bands are not wired up yet — this currently hands the developed signal
// through unchanged.

@group(0) @binding(0) var developedTexture: texture_2d<f32>;

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
  // The developed texture is the same size as this target, so fragment
  // coordinates index it directly. Loading rather than sampling keeps the
  // mapping exact — no filtering is wanted and no half-texel offset can creep
  // in between the stages.
  return textureLoad(developedTexture, vec2i(position.xy), 0);
}
