// src/lib/gpu/shaders/chromaticityDraw.wgsl
//
// The CIE 1931 diagram: the filled spectral locus, the image's chromaticities
// as a cloud coloured by each cell's share of the pixels, and a triangle for
// every enabled gamut.

//!include "chunks/luma.wgsl"
//!include "chunks/tone.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(0) var<storage, read> xyBins:  array<u32, XY_TOTAL>;
@group(0) @binding(1) var<storage, read> stats:   array<u32, STAT_WORDS>;
@group(0) @binding(2) var<storage, read> overlay: GamutOverlay;

// The locus is dimmed so the data cloud stands out over it.
const LOCUS_LEVEL: f32 = 0.4;
const LOCUS_FILL: f32 = 0.95;

const GRID_TINT = vec3f(0.42, 0.44, 0.48);
// A cell's share of the measured pixels, by decade: 10^-6 at the bottom of the
// ramp, 10^-1 at the top. Mirrored by the legend in ColorSpacePanel.svelte.
const SHARE_DECADE_MIN: f32 = -6.0;
const SHARE_DECADES: f32 = 5.0;

// Opacity of the rarest cell, so a lone pixel stays visible but recedes.
const CLOUD_FLOOR: f32 = 0.7;

struct VertexOut {
  @builtin(position) position: vec4f,
  // (0, 0) at the bottom left of the plot, (1, 1) at the top right.
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

const PLOT_SPAN = vec2f(XY_X_MAX, XY_Y_MAX);

fn to_uv(xy: vec2f) -> vec2f {
  return xy / PLOT_SPAN;
}

// Whether a point falls inside the spectral locus, by crossing number. The
// closing segment is the line of purples.
fn inside_locus(uv: vec2f) -> bool {
  var inside = false;
  var previous = to_uv(overlay.locus[LOCUS_POINTS - 1u]);
  for (var i = 0u; i < LOCUS_POINTS; i++) {
    let current = to_uv(overlay.locus[i]);
    let straddles = (current.y > uv.y) != (previous.y > uv.y);
    if (straddles) {
      let t = (uv.y - current.y) / (previous.y - current.y);
      if (uv.x < current.x + t * (previous.x - current.x)) {
        inside = !inside;
      }
    }
    previous = current;
  }
  return inside;
}

// The colour of a chromaticity at the brightest the display holds it.
// Chromaticities outside sRGB are clipped towards its boundary.
fn locus_colour(uv: vec2f) -> vec3f {
  let xy = uv * PLOT_SPAN;
  if (xy.y < COLORIMETRY_EPSILON) { return vec3f(0.0); }

  let xyz = vec3f(xy.x / xy.y, 1.0, (1.0 - xy.x - xy.y) / xy.y);
  let display = max(M_WORKING_TO_SRGB * (M_XYZ_TO_WORKING * xyz), vec3f(0.0));
  let peak = max(display.r, max(display.g, display.b));
  if (peak < COLORIMETRY_EPSILON) { return vec3f(0.0); }

  return encode_srgb(display / peak);
}

fn segment_distance(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let ab = b - a;
  let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-9), 0.0, 1.0);
  return length(p - (a + ab * t));
}

// A line of `width` pixels, antialiased across one.
fn stroke(distance: f32, width: f32, px: f32) -> f32 {
  return 1.0 - smoothstep(width * px * 0.5, width * px * 0.5 + px, distance);
}

// Distance to a triangle's edges, in xy units.
fn triangle_edge(space: GamutSpace, xy: vec2f) -> f32 {
  return min(
    segment_distance(space.red, space.green, xy),
    min(segment_distance(space.green, space.blue, xy), segment_distance(space.blue, space.red, xy)),
  );
}

fn unpack_tint(packed: u32) -> vec3f {
  return vec3f(
    f32((packed >> 16u) & 255u),
    f32((packed >> 8u) & 255u),
    f32(packed & 255u),
  ) / 255.0;
}

// Violet for the rarest cells through to near white for the most common.
fn heat(t: f32) -> vec3f {
  let c0 = vec3f(0.36, 0.22, 0.85);
  let c1 = vec3f(0.85, 0.22, 0.55);
  let c2 = vec3f(0.98, 0.50, 0.15);
  let c3 = vec3f(1.00, 0.98, 0.82);
  if (t < 1.0 / 3.0) { return mix(c0, c1, t * 3.0); }
  if (t < 2.0 / 3.0) { return mix(c1, c2, t * 3.0 - 1.0); }
  return mix(c2, c3, t * 3.0 - 2.0);
}

// A cell's colour and coverage, premultiplied, from its share of the pixels.
fn cell_cloud(cell: vec2i) -> vec4f {
  if (cell.x < 0 || cell.y < 0 || cell.x >= i32(XY_BINS) || cell.y >= i32(XY_BINS)) {
    return vec4f(0.0);
  }
  let count = xyBins[u32(cell.y) * XY_BINS + u32(cell.x)];
  if (count == 0u) { return vec4f(0.0); }

  let share = f32(count) / f32(max(stats[STAT_COUNTED], 1u));
  let t = clamp((log(share) / log(10.0) - SHARE_DECADE_MIN) / SHARE_DECADES, 0.0, 1.0);
  let a = mix(CLOUD_FLOOR, 1.0, t);
  return vec4f(heat(t) * a, a);
}

// Bilinear between cells, so the cloud does not read as a grid.
fn cloud_at(uv: vec2f) -> vec4f {
  let t = uv * f32(XY_BINS) - vec2f(0.5);
  let base = vec2i(floor(t));
  let f = fract(t);

  return mix(
    mix(cell_cloud(base), cell_cloud(base + vec2i(1, 0)), f.x),
    mix(cell_cloud(base + vec2i(0, 1)), cell_cloud(base + vec2i(1, 1)), f.x),
    f.y,
  );
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  // Lines are measured in xy, whose units are square on the 8 : 9 canvas.
  let xy = in.uv * PLOT_SPAN;
  let px = max(fwidth(xy.x), 1e-5);

  var colour = vec3f(0.0);
  var alpha = 0.0;

  if (inside_locus(in.uv)) {
    colour = locus_colour(in.uv) * LOCUS_LEVEL;
    alpha = LOCUS_FILL;
  }

  // Grid lines every 0.1.
  var grid = 0.0;
  for (var i = 1u; i < 9u; i++) {
    let at = f32(i) * 0.1;
    grid = max(grid, stroke(abs(xy.x - at), 0.6, px));
    grid = max(grid, stroke(abs(xy.y - at), 0.6, px));
  }
  colour = mix(colour, GRID_TINT, grid * 0.35);
  alpha = max(alpha, grid * 0.25);

  let cloud = cloud_at(in.uv);
  if (cloud.a > 0.0) {
    colour = mix(colour, cloud.rgb / cloud.a, cloud.a);
    alpha = max(alpha, cloud.a);
  }

  // Gamut triangles, drawn over everything else.
  for (var s = 0u; s < GAMUT_SPACES; s++) {
    let space = overlay.spaces[s];
    if (space.enabled == 0u) { continue; }

    let line = stroke(triangle_edge(space, xy), 1.4, px);
    let tint = unpack_tint(space.tint);
    colour = mix(colour, tint, line);
    alpha = max(alpha, line);
  }

  // The D65 white point.
  let white = stroke(length(xy - vec2f(0.3127, 0.3290)), 3.0, px);
  colour = mix(colour, vec3f(1.0), white);
  alpha = max(alpha, white);

  return vec4f(colour * alpha, alpha);
}
