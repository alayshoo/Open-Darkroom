// src/lib/gpu/shaders/retouchSource.wgsl
//
// Scores where a stroke could take its pixels from: for each candidate offset,
// how unlike the ring around the stroke the same ring moved by the offset is.
// A heal corrects the tone itself, so for a heal only the spread of the
// difference counts and not its mean.

struct Search {
  samples:    u32,
  candidates: u32,
  heal:       u32,
  _pad:       u32,
  image_size: vec2f,
  _pad2:      vec2f,
}

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> search: Search;
@group(0) @binding(2) var<storage, read> samples: array<vec2f>;
@group(0) @binding(3) var<storage, read> candidates: array<vec2f>;
@group(0) @binding(4) var<storage, read_write> scores: array<f32>;

const EPS: f32 = 1e-6;

fn log_at(t: vec2f) -> vec3f {
  return log(max(textureLoad(image, vec2i(t), 0).rgb, vec3f(EPS)));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= search.candidates) {
    return;
  }
  let offset = candidates[id.x];
  var sum = vec3f(0.0);
  var squares = vec3f(0.0);
  var n = 0.0;
  for (var i = 0u; i < search.samples; i++) {
    let p = samples[i];
    let s = p + offset;
    if (any(s < vec2f(0.0)) || any(s >= search.image_size)) {
      continue;
    }
    let e = log_at(p) - log_at(s);
    sum += e;
    squares += e * e;
    n += 1.0;
  }
  if (n < 0.5 * f32(search.samples) || n == 0.0) {
    scores[id.x] = 1e30;
    return;
  }
  let mean = sum / n;
  var spread = squares / n;
  if (search.heal == 1u) {
    spread -= mean * mean;
  }
  scores[id.x] = spread.x + spread.y + spread.z;
}
