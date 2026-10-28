// src-native/src/gpu/chrome.wgsl
//
// The window chrome the Tauri app draws with CSS: rounded fills, gradients,
// box shadows, the frosted glass material and the scrim along the window
// edges. Each primitive is one instanced quad over its own bounds, shaded
// from a signed distance to its rounded rectangle. Everything is in physical
// pixels and premultiplied, as egui blends.

struct Globals {
  screen: vec2f,
  ppp: f32,
  scrim: f32,
  // The glass backdrop's CSS filters.
  saturate: f32,
  brightness: f32,
  _pad: vec2f,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var backdrop: texture_2d<f32>;
@group(0) @binding(2) var backdrop_sampler: sampler;

const FILL: u32 = 0u;
const LINEAR: u32 = 1u;
const SHADOW: u32 = 2u;
const INSET: u32 = 3u;
const GLASS: u32 = 4u;
const SCRIM: u32 = 5u;
const RING: u32 = 6u;

struct Prim {
  @location(0) bounds: vec4f,
  @location(1) rect: vec4f,
  @location(2) radii: vec4f,
  @location(3) color: vec4f,
  @location(4) color2: vec4f,
  @location(5) p0: vec4f,
  @location(6) p1: vec4f,
  @location(7) mode: u32,
}

struct VOut {
  @builtin(position) position: vec4f,
  @location(0) pixel: vec2f,
  @location(1) @interpolate(flat) rect: vec4f,
  @location(2) @interpolate(flat) radii: vec4f,
  @location(3) @interpolate(flat) color: vec4f,
  @location(4) @interpolate(flat) color2: vec4f,
  @location(5) @interpolate(flat) p0: vec4f,
  @location(6) @interpolate(flat) p1: vec4f,
  @location(7) @interpolate(flat) mode: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, prim: Prim) -> VOut {
  let corner = vec2f(f32(vi & 1u), f32(vi >> 1u));
  let pixel = mix(prim.bounds.xy, prim.bounds.zw, corner);
  var out: VOut;
  out.position = vec4f(pixel / globals.screen * vec2f(2.0, -2.0) + vec2f(-1.0, 1.0), 0.0, 1.0);
  out.pixel = pixel;
  out.rect = prim.rect;
  out.radii = prim.radii;
  out.color = prim.color;
  out.color2 = prim.color2;
  out.p0 = prim.p0;
  out.p1 = prim.p1;
  out.mode = prim.mode;
  return out;
}

// Signed distance to a rounded rectangle, negative inside. Radii run
// top-left, top-right, bottom-right, bottom-left, as CSS lists them.
fn sd_rrect(p: vec2f, rect: vec4f, radii: vec4f) -> f32 {
  let center = (rect.xy + rect.zw) * 0.5;
  let half = (rect.zw - rect.xy) * 0.5;
  let q = p - center;
  var r = select(radii.w, radii.z, q.x > 0.0);
  if (q.y < 0.0) {
    r = select(radii.x, radii.y, q.x > 0.0);
  }
  r = min(r, min(half.x, half.y));
  let d = abs(q) - half + r;
  return length(max(d, vec2f(0.0))) + min(max(d.x, d.y), 0.0) - r;
}

fn coverage(p: vec2f, rect: vec4f, radii: vec4f) -> f32 {
  return clamp(0.5 - sd_rrect(p, rect, radii), 0.0, 1.0);
}

fn erf7(x: f32) -> f32 {
  let xx = x * x;
  let y = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
  return y / sqrt(1.0 + y * y);
}

// A rounded rectangle blurred by a Gaussian of `sigma`, after Raph Levien's
// closed-form approximation.
fn blurred_rrect(p: vec2f, rect: vec4f, radius: f32, sigma: f32) -> f32 {
  let size = rect.zw - rect.xy;
  if (sigma < 0.25) {
    return coverage(p, rect, vec4f(radius));
  }
  let center = (rect.xy + rect.zw) * 0.5;
  let s_inv = 1.0 / (sigma * 1.41421356);
  let min_edge = min(size.x, size.y);
  let r_max = 0.5 * min_edge;
  let r0 = min(length(vec2f(radius, 1.15 * sigma)), r_max);
  let r1 = min(length(vec2f(radius, 2.0 * sigma)), r_max);
  let exponent = 2.0 * r1 / max(r0, 1e-3);
  let delta = 1.25 * sigma * (exp(-pow(0.5 * size.x / sigma, 2.0)) - exp(-pow(0.5 * size.y / sigma, 2.0)));
  let w = size.x + min(delta, 0.0);
  let h = size.y - max(delta, 0.0);
  let scale = 0.5 * erf7(s_inv * 0.5 * (max(w, h) - 0.5 * radius));
  let q = abs(p - center) - vec2f(0.5 * w - r0, 0.5 * h - r0);
  let qp = max(q, vec2f(0.0));
  let d_pos = pow(pow(qp.x, exponent) + pow(qp.y, exponent), 1.0 / exponent);
  let d_neg = min(max(q.x, q.y), 0.0);
  let d = d_pos + d_neg - r0;
  return clamp(scale * (erf7(s_inv * (min_edge + d)) - erf7(s_inv * d)), 0.0, 1.0);
}

// Where p falls along a CSS linear-gradient of `deg` across `rect`.
fn gradient_t(p: vec2f, rect: vec4f, deg: f32) -> f32 {
  let a = radians(deg);
  let dir = vec2f(sin(a), -cos(a));
  let size = rect.zw - rect.xy;
  let len = abs(size.x * dir.x) + abs(size.y * dir.y);
  return dot(p - (rect.xy + rect.zw) * 0.5, dir) / max(len, 1e-3) + 0.5;
}

fn ramp4(t: f32, a: f32, b: f32, c: f32, d: f32, sb: f32, sc: f32) -> f32 {
  if (t <= 0.0) { return a; }
  if (t < sb) { return mix(a, b, t / sb); }
  if (t < sc) { return mix(b, c, (t - sb) / (sc - sb)); }
  if (t < 1.0) { return mix(c, d, (t - sc) / (1.0 - sc)); }
  return d;
}

// The scrim's smoothstep ramp: transparent at 0, black at 1.
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

// One band of the scrim, in CSS pixels: down the left edge `left` deep and
// across the top `top` deep, turning the corner on an elliptical tile.
fn scrim_band(p: vec2f, top: f32, left: f32, reach: vec2f) -> f32 {
  if (p.x < reach.x && p.y < reach.y) {
    let rho = length((p - reach) / reach);
    return scrim_ramp((rho - 0.3125) / 0.6875);
  }
  if (p.y < reach.y) {
    return scrim_ramp(1.0 - p.y / top);
  }
  return scrim_ramp(1.0 - p.x / left);
}

fn scrim_alpha(px: vec2f) -> f32 {
  let p = px / globals.ppp;
  let dense = scrim_band(p, 55.0, 68.75, vec2f(100.0, 80.0)) * globals.scrim;
  let wide = scrim_band(p, 143.0, 178.75, vec2f(260.0, 208.0)) * globals.scrim * 0.35;
  return 1.0 - (1.0 - dense) * (1.0 - wide);
}

fn hash(p: vec2f) -> f32 {
  let h = dot(p, vec2f(127.1, 311.7));
  return fract(sin(h) * 43758.5453);
}

// The grain the CSS draws from an feTurbulence tile at 5%.
fn grain(px: vec2f) -> f32 {
  let p = floor(px / globals.ppp * 0.75);
  return hash(p) * 0.6 + hash(floor(p * 0.5) + 17.0) * 0.4;
}

fn saturate_filter(c: vec3f, s: f32) -> vec3f {
  let m = mat3x3f(
    vec3f(0.213 + 0.787 * s, 0.213 - 0.213 * s, 0.213 - 0.213 * s),
    vec3f(0.715 - 0.715 * s, 0.715 + 0.285 * s, 0.715 - 0.715 * s),
    vec3f(0.072 - 0.072 * s, 0.072 - 0.072 * s, 0.072 + 0.928 * s),
  );
  return m * c;
}

fn over(top: vec4f, under: vec4f) -> vec4f {
  return top + under * (1.0 - top.a);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4f {
  let p = in.pixel;
  switch in.mode {
    case FILL: {
      return in.color * coverage(p, in.rect, in.radii);
    }
    case LINEAR: {
      // p0.xy to p0.zw is the gradient line; color to color2 along it.
      let line = in.p0.zw - in.p0.xy;
      let t = clamp(dot(p - in.p0.xy, line) / max(dot(line, line), 1e-6), 0.0, 1.0);
      return mix(in.color, in.color2, t) * coverage(p, in.rect, in.radii);
    }
    case SHADOW: {
      // p0: offset, blur, spread. Cut out under the element, as CSS clips it.
      let spread = in.p0.w;
      let shadow = vec4f(in.rect.xy + in.p0.xy - spread, in.rect.zw + in.p0.xy + spread);
      let a = blurred_rrect(p, shadow, max(in.radii.x + spread, 0.0), in.p0.z * 0.5);
      return in.color * a * (1.0 - coverage(p, in.rect, in.radii));
    }
    case INSET: {
      // p0: offset, blur, spread. The hole the shadow is cast around.
      let spread = in.p0.w;
      let hole = vec4f(in.rect.xy + in.p0.xy + spread, in.rect.zw + in.p0.xy - spread);
      let a = 1.0 - blurred_rrect(p, hole, max(in.radii.x - spread, 0.0), in.p0.z * 0.5);
      return in.color * a * coverage(p, in.rect, in.radii);
    }
    case GLASS: {
      // p0: angle, rim stops hi, mid, lo. p1: rim end, backdrop on, gloss at
      // 0% and 33%. color: the tint. color2.x: how much of the backdrop
      // shows, for a surface under a dimming scrim; color2.y: its opacity.
      let cover = coverage(p, in.rect, in.radii);
      if (cover <= 0.0) {
        discard;
      }
      var surface = vec4f(0.0);
      if (in.p1.y > 0.5) {
        let uv = p / globals.screen;
        var b = textureSampleLevel(backdrop, backdrop_sampler, uv, 0.0).rgb;
        b = saturate_filter(b, globals.saturate) * globals.brightness * in.color2.x;
        surface = vec4f(clamp(b, vec3f(0.0), vec3f(1.0)), 1.0);
      }
      surface = over(in.color, surface);
      surface = over(vec4f(vec3f(grain(p)), 1.0) * 0.025, surface);

      let t = gradient_t(p, in.rect, in.p0.x);
      let gloss = ramp4(t, in.p1.z, in.p1.w, 0.0, 0.0, 0.33, 0.55);
      surface = over(vec4f(gloss), surface);

      let inner = vec4f(in.rect.xy + globals.ppp, in.rect.zw - globals.ppp);
      let ring = cover - coverage(p, inner, max(in.radii - globals.ppp, vec4f(0.0)));
      let rim = ramp4(t, in.p0.y, in.p0.z, in.p0.w, in.p1.x, 0.30, 0.62) * ring;
      surface = over(vec4f(rim), surface);
      return surface * cover * in.color2.y;
    }
    case SCRIM: {
      return vec4f(0.0, 0.0, 0.0, scrim_alpha(p));
    }
    case RING: {
      // p0.x: the stroke's width, inside the rect.
      let w = in.p0.x;
      let inner = vec4f(in.rect.xy + w, in.rect.zw - w);
      let a = coverage(p, in.rect, in.radii) - coverage(p, inner, max(in.radii - w, vec4f(0.0)));
      return in.color * clamp(a, 0.0, 1.0);
    }
    default: {
      return vec4f(0.0);
    }
  }
}
