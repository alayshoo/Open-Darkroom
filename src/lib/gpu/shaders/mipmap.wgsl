// src/lib/gpu/shaders/mipmap.wgsl
//
// One mip level of the full-resolution source from the level above it. The
// source is linear light, so averaging its texels averages the light itself.

@group(0) @binding(0) var parentLevel: texture_2d<f32>;
@group(0) @binding(1) var parentSampler: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> @builtin(position) vec4f {
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

// A level is half its parent, rounded down, so on an even parent each texel
// lands between four parent texels and the bilinear tap is their mean.
@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let size = max(textureDimensions(parentLevel) / 2u, vec2u(1u));
  return textureSampleLevel(parentLevel, parentSampler, position.xy / vec2f(size), 0.0);
}
