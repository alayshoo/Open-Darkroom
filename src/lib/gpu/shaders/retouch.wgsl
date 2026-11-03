// src/lib/gpu/shaders/retouch.wgsl
//
// Heal and clone strokes, applied to the linear source before anything else
// reads it. Run tile by tile by the preview (retouchLayer.ts) and the export
// (retouch.rs); every pass works in texels of the image being retouched.
//
// Clone copies the source pixels as they are. Heal copies them and corrects
// their tone towards the pixels around the stroke: the ratio between the two
// neighbourhoods, in log space, is spread in from outside the stroke by a
// normalised Gaussian, so the grain comes from the source and the tone from
// the destination.
//
//   fs_prep    log(destination / source) and a weight, zero under the stroke
//   fs_blur_h  the two blurred along rows
//   fs_blur_v  and along columns
//   fs_apply   the stroke laid over the image

struct RetouchPass {
  // Image texel of the target's first texel.
  target_origin: vec2f,
  // Image texel of the work textures' first texel, and how much of them is used.
  work_origin:   vec2f,
  work_size:     vec2f,
  // Image texel of `image`'s first texel, and the size of the whole image.
  image_origin:  vec2f,
  image_size:    vec2f,
  // Whole texels from a stroke texel to the one it takes its pixels from.
  offset:        vec2f,
  radius:        f32,
  feather:       f32,
  opacity:       f32,
  sigma:         f32,
  step:          f32,
  taps:          u32,
  segments:      u32,
  heal:          u32,
}

@group(0) @binding(0) var image: texture_2d<f32>;
// What fs_blur_h and fs_blur_v blur, and what fs_apply reads the blurred match from.
@group(0) @binding(1) var work: texture_2d<f32>;
@group(0) @binding(2) var<uniform> pass_: RetouchPass;
// Segments of the stroke's centre line, [ax, ay, bx, by] in image texels.
@group(0) @binding(3) var<storage, read> segments: array<vec4f>;

const EPS: f32 = 1e-6;

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

fn stroke_distance(p: vec2f) -> f32 {
  var d = 1e30;
  for (var i = 0u; i < pass_.segments; i++) {
    let s = segments[i];
    let ba = s.zw - s.xy;
    let h = clamp(dot(p - s.xy, ba) / max(dot(ba, ba), 1e-12), 0.0, 1.0);
    d = min(d, length(p - s.xy - ba * h));
  }
  return d;
}

// 1 across the stroke, falling to 0 at its radius. Even a hard edge fades over
// a texel, so it does not alias.
fn stroke_mask(d: f32) -> f32 {
  let soft = max(pass_.radius * pass_.feather, 1.0);
  return 1.0 - smoothstep(pass_.radius - soft, pass_.radius, d);
}

fn inside_image(t: vec2f) -> bool {
  return all(t >= vec2f(0.0)) && all(t < pass_.image_size);
}

fn load_image(t: vec2f) -> vec4f {
  return textureLoad(image, vec2i(t - pass_.image_origin), 0);
}

fn log_rgb(c: vec3f) -> vec3f {
  return log(max(c, vec3f(EPS)));
}

@fragment
fn fs_prep(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let t = pass_.work_origin + floor(position.xy);
  let s = t + pass_.offset;
  // Only what lies clear of the stroke says what its surroundings look like.
  if (!inside_image(s) || stroke_distance(t + 0.5) < pass_.radius + 1.0) {
    return vec4f(0.0);
  }
  return vec4f(log_rgb(load_image(t).rgb) - log_rgb(load_image(s).rgb), 1.0);
}

fn blur(position: vec4f, axis: vec2i) -> vec4f {
  let p = vec2i(floor(position.xy));
  let size = vec2i(pass_.work_size);
  let k = -0.5 / (pass_.sigma * pass_.sigma);
  var sum = vec4f(0.0);
  for (var i = -i32(pass_.taps); i <= i32(pass_.taps); i++) {
    let along = f32(i) * pass_.step;
    let q = p + axis * i32(along);
    if (any(q < vec2i(0)) || any(q >= size)) {
      continue;
    }
    sum += exp(k * along * along) * textureLoad(work, q, 0);
  }
  return sum;
}

@fragment
fn fs_blur_h(@builtin(position) position: vec4f) -> @location(0) vec4f {
  return blur(position, vec2i(1, 0));
}

@fragment
fn fs_blur_v(@builtin(position) position: vec4f) -> @location(0) vec4f {
  return blur(position, vec2i(0, 1));
}

@fragment
fn fs_apply(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let t = pass_.target_origin + floor(position.xy);
  let destination = load_image(t);
  let s = t + pass_.offset;
  let amount = stroke_mask(stroke_distance(t + 0.5)) * pass_.opacity;
  if (amount <= 0.0 || !inside_image(s)) {
    return destination;
  }

  var taken = load_image(s).rgb;
  if (pass_.heal == 1u) {
    let matched = textureLoad(work, vec2i(t - pass_.work_origin), 0);
    let shift = select(vec3f(0.0), matched.rgb / matched.a, matched.a > EPS);
    taken = exp(log_rgb(taken) + shift);
  }
  return vec4f(mix(destination.rgb, taken, amount), destination.a);
}
