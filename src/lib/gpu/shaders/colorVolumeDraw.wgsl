// src/lib/gpu/shaders/colorVolumeDraw.wgsl
//
// The colour volume: hue across the plot, lightness up it, and in every cell
// the chroma its pixels actually reached, drawn as the colour that describes.
//
// It answers what the chromaticity diagram cannot. A triangle in xy says
// nothing about brightness, so two images with the same hues at wildly
// different exposures plot identically there — here they do not, and a hue the
// edit only reaches in the shadows reads differently from one it carries all
// the way up.

//!include "chunks/tone.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(0) var<storage, read> hlBins: array<u32, HL_TOTAL>;
@group(0) @binding(1) var<storage, read> scale:  array<u32, 2>;

const GRID_TINT = vec3f(0.42, 0.44, 0.48);

// Floor under a cell that holds anything at all, so a hue carried by a handful
// of pixels is visible rather than merely present.
const PRESENT_FLOOR: f32 = 0.30;

struct VertexOut {
  @builtin(position) position: vec4f,
  // Hue across, lightness up.
  @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> VertexOut {
  var pos = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f( 1.0, -1.0),
    vec2f(-1.0,  1.0),
    vec2f(-1.0,  1.0),
    vec2f( 1.0, -1.0),
    vec2f( 1.0,  1.0),
  );
  let p = pos[vertexIndex];

  var out: VertexOut;
  out.position = vec4f(p, 0.0, 1.0);
  out.uv = p * 0.5 + vec2f(0.5);
  return out;
}

// A cell's coverage and its mean chroma. Hue wraps — red is one cell from
// magenta — while lightness does not.
fn cell(hue: i32, light: i32) -> vec2f {
  if (light < 0 || light >= i32(LIGHT_BINS)) { return vec2f(0.0); }
  let h = u32((hue + i32(HUE_BINS)) % i32(HUE_BINS));

  let base = (u32(light) * HUE_BINS + h) * 2u;
  let count = hlBins[base + HL_COUNT];
  if (count == 0u) { return vec2f(0.0); }

  let coverage = PRESENT_FLOOR
    + (1.0 - PRESENT_FLOOR) * log(1.0 + f32(count)) / log(1.0 + f32(scale[1]));
  let chroma = f32(hlBins[base + HL_CHROMA]) / (f32(count) * CHROMA_SCALE);
  return vec2f(coverage, chroma);
}

// Read between cells, so the plot describes a volume rather than a mosaic. The
// chroma is weighted by coverage: an empty neighbour has no chroma to average
// in, and letting its zero pull the mean down would bleed grey into every edge.
fn sample_cell(uv: vec2f) -> vec2f {
  let t = vec2f(uv.x * f32(HUE_BINS), uv.y * f32(LIGHT_BINS)) - vec2f(0.5);
  let base = vec2i(floor(t));
  let f = fract(t);

  var coverage = 0.0;
  var weighted = 0.0;
  for (var dy = 0; dy < 2; dy++) {
    for (var dx = 0; dx < 2; dx++) {
      let c = cell(base.x + dx, base.y + dy);
      let w = mix(1.0 - f.x, f.x, f32(dx)) * mix(1.0 - f.y, f.y, f32(dy));
      coverage += c.x * w;
      weighted += c.x * c.y * w;
    }
  }

  return vec2f(coverage, select(0.0, weighted / coverage, coverage > 1e-5));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let px = max(fwidth(in.uv.x), 1e-5);

  // The quarters of lightness, and the twelve hue sectors — the latter in the
  // hue they stand for, which is the only thing orienting the horizontal axis.
  var grid = 0.0;
  for (var i = 1u; i < 4u; i++) {
    grid = max(grid, 1.0 - smoothstep(0.0, px, abs(in.uv.y - f32(i) / 4.0)));
  }

  var colour = GRID_TINT;
  var alpha = grid * 0.28;

  for (var i = 1u; i < 12u; i++) {
    let hue = f32(i) / 12.0;
    let line = 1.0 - smoothstep(0.0, px, abs(in.uv.x - hue));
    if (line > 0.0) {
      let swatch = oklab_to_working(lch_to_oklab(vec3f(0.72, 0.12, hue)));
      colour = encode_srgb(clamp(swatch, vec3f(0.0), vec3f(1.0)));
      alpha = max(alpha, line * 0.34);
    }
  }

  let sampled = sample_cell(in.uv);
  if (sampled.x > 0.0) {
    let lightness = in.uv.y * LIGHT_MAX;
    let working = oklab_to_working(lch_to_oklab(vec3f(lightness, sampled.y, in.uv.x)));

    // Clamped, not gamut-compressed: this is a swatch of one colour rather than
    // a picture, and the honest thing for a chroma the display cannot hold is
    // the nearest it can.
    colour = encode_srgb(clamp(working, vec3f(0.0), vec3f(1.0)));
    alpha = max(alpha, sampled.x);
  }

  return vec4f(colour * alpha, alpha);
}
