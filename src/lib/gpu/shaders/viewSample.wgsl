// src/lib/gpu/shaders/viewSample.wgsl
//
// The first stage of the preview: resamples the source into the region of the
// image the chain is rendering, at the chain's render scale. The source carries
// mip levels, and the sampler blends between them by how far it is shrunk.

@group(0) @binding(0) var sourceTexture: texture_2d<f32>;
@group(0) @binding(1) var sourceSampler: sampler;
@group(0) @binding(2) var<uniform> view: ViewSample;

// The source texel coordinate of output point p is
// origin + p.x * axis_x + p.y * axis_y.
struct ViewSample {
  origin: vec2f,
  axis_x: vec2f,
  axis_y: vec2f,
  // 1 when the map steps one whole texel per pixel from a texel centre, so
  // the source is copied rather than filtered.
  exact:  f32,
  _pad:   f32,
}

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

// Past the edges of the image both paths clamp, so the blurs downstream see its
// border repeated, as they do on the image itself in the export.
@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let dims = textureDimensions(sourceTexture);
  let src = view.origin + position.x * view.axis_x + position.y * view.axis_y;
  let filtered = textureSample(sourceTexture, sourceSampler, src / vec2f(dims));
  if (view.exact > 0.5) {
    let texel = clamp(vec2i(floor(src)), vec2i(0), vec2i(dims) - 1);
    return textureLoad(sourceTexture, texel, 0);
  }
  return filtered;
}
