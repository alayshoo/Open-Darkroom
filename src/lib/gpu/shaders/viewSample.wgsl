// src/lib/gpu/shaders/viewSample.wgsl
//
// The first stage of the preview: resamples whichever pyramid level is loaded
// into a texture laid out like the screen, so the rest of the chain runs once
// per pixel on screen whatever the zoom.

@group(0) @binding(0) var sourceTexture: texture_2d<f32>;
@group(0) @binding(1) var sourceSampler: sampler;
@group(0) @binding(2) var<uniform> view: ViewSample;

// The source texel coordinate of output point p is
// origin + p.x * axis_x + p.y * axis_y.
struct ViewSample {
  origin: vec2f,
  axis_x: vec2f,
  axis_y: vec2f,
  _pad:   vec2f,
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

// Clamped to the outermost texel centres, so past the edge of the image the
// blurs downstream see its border repeated, as they would on the image itself.
@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let size = vec2f(textureDimensions(sourceTexture));
  let src = view.origin + position.x * view.axis_x + position.y * view.axis_y;
  return textureSampleLevel(sourceTexture, sourceSampler, clamp(src, vec2f(0.5), size - 0.5) / size, 0.0);
}
