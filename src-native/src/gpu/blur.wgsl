// src-native/src/gpu/blur.wgsl
//
// The frosted backdrop the glass samples: the window behind the panels —
// backdrop colour, image and scrim — taken down to a quarter and blurred, as
// CSS backdrop-filter blurs what is behind an element.

struct Params {
  backdrop: vec4f,
  // Texel step of the source, and the Gaussian's sigma in target texels.
  texel: vec2f,
  sigma: f32,
  ppp: f32,
  scrim: f32,
  target_size_x: f32,
  target_size_y: f32,
  _pad: f32,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
  var pos = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return vec4f(pos[vi], 0.0, 1.0);
}

fn scrim_ramp(t: f32) -> f32 {
  let stops = array<f32, 8>(0.0, 0.06, 0.22, 0.42, 0.65, 0.84, 0.97, 1.0);
  let at = array<f32, 8>(0.0, 0.15, 0.30, 0.45, 0.60, 0.75, 0.90, 1.0);
  if (t <= 0.0) { return 0.0; }
  if (t >= 1.0) { return 1.0; }
  for (var i = 1u; i < 8u; i++) {
    if (t <= at[i]) {
      return mix(stops[i - 1u], stops[i], (t - at[i - 1u]) / (at[i] - at[i - 1u]));
    }
  }
  return 1.0;
}

fn scrim_band(p: vec2f, top: f32, left: f32, reach: vec2f) -> f32 {
  if (p.x < reach.x && p.y < reach.y) {
    return scrim_ramp((length((p - reach) / reach) - 0.3125) / 0.6875);
  }
  if (p.y < reach.y) {
    return scrim_ramp(1.0 - p.y / top);
  }
  return scrim_ramp(1.0 - p.x / left);
}

// The window at half resolution: four canvas texels, over the backdrop colour,
// under the scrim.
@fragment
fn compose(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let uv = position.xy / vec2f(params.target_size_x, params.target_size_y);
  let image = textureSampleLevel(source, source_sampler, uv, 0.0);
  var rgb = image.rgb + params.backdrop.rgb * (1.0 - image.a);
  let p = position.xy * 2.0 / params.ppp;
  let dense = scrim_band(p, 55.0, 68.75, vec2f(100.0, 80.0)) * params.scrim;
  let wide = scrim_band(p, 143.0, 178.75, vec2f(260.0, 208.0)) * params.scrim * 0.35;
  rgb *= (1.0 - dense) * (1.0 - wide);
  return vec4f(rgb, 1.0);
}

@fragment
fn downsample(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let uv = position.xy / vec2f(params.target_size_x, params.target_size_y);
  return textureSampleLevel(source, source_sampler, uv, 0.0);
}

fn gaussian(position: vec4f, dir: vec2f) -> vec4f {
  let size = vec2f(params.target_size_x, params.target_size_y);
  let radius = i32(ceil(params.sigma * 3.0));
  var total = vec4f(0.0);
  var weight = 0.0;
  for (var i = -radius; i <= radius; i++) {
    let w = exp(-f32(i * i) / (2.0 * params.sigma * params.sigma));
    let uv = (position.xy + dir * f32(i)) / size;
    total += textureSampleLevel(source, source_sampler, uv, 0.0) * w;
    weight += w;
  }
  return total / weight;
}

@fragment
fn blur_h(@builtin(position) position: vec4f) -> @location(0) vec4f {
  return gaussian(position, vec2f(1.0, 0.0));
}

@fragment
fn blur_v(@builtin(position) position: vec4f) -> @location(0) vec4f {
  return gaussian(position, vec2f(0.0, 1.0));
}
