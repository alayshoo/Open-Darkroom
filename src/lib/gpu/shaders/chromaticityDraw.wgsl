// src/lib/gpu/shaders/chromaticityDraw.wgsl
//
// The CIE 1931 diagram: the spectral locus filled with the colour of each
// chromaticity, the image's own chromaticities as a density cloud over it, and
// a triangle for every space the panel has switched on.
//
// There is no geometry. Every fragment asks where it falls — inside the locus,
// near an edge, how full its cell is — which is the same arrangement
// histogramDraw.wgsl uses, and it keeps the overlays out of a vertex buffer
// that would have to be rebuilt whenever a checkbox moved.

//!include "chunks/tone.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(0) var<storage, read> xyBins:  array<u32, XY_TOTAL>;
@group(0) @binding(1) var<storage, read> scale:   array<u32, 2>;
@group(0) @binding(2) var<storage, read> overlay: GamutOverlay;

// The locus is a backdrop: the data cloud sits on top of it and has to win, so
// it is drawn at a fraction of full brightness rather than at the strength the
// printed diagram uses.
const LOCUS_LEVEL: f32 = 0.62;
const LOCUS_FILL: f32 = 0.95;

const GRID_TINT = vec3f(0.42, 0.44, 0.48);
const DATA_TINT = vec3f(1.0, 0.96, 0.90);

struct VertexOut {
  @builtin(position) position: vec4f,
  // Across the plot and up it, so (0, 0) is x = 0, y = 0 at the bottom left.
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
// closing segment is the line of purples, which is as much a boundary of real
// colour as the curve itself.
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

// The colour of a chromaticity, taken at the brightest the working space can
// hold it. Anything outside Rec.2020 has a negative channel and desaturates
// towards the edge of the fill rather than inventing a colour for it.
fn locus_colour(uv: vec2f) -> vec3f {
  let xy = uv * PLOT_SPAN;
  if (xy.y < COLORIMETRY_EPSILON) { return vec3f(0.0); }

  let xyz = vec3f(xy.x / xy.y, 1.0, (1.0 - xy.x - xy.y) / xy.y);
  let working = max(M_XYZ_TO_WORKING * xyz, vec3f(0.0));
  let peak = max(working.r, max(working.g, working.b));
  if (peak < COLORIMETRY_EPSILON) { return vec3f(0.0); }

  return encode_srgb(working / peak);
}

// Distance from a point to a segment, in plot units.
fn segment_distance(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let ab = b - a;
  let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-9), 0.0, 1.0);
  return length(p - (a + ab * t));
}

// A line of `width` pixels, antialiased across one.
fn stroke(distance: f32, width: f32, px: f32) -> f32 {
  return 1.0 - smoothstep(width * px * 0.5, width * px * 0.5 + px, distance);
}

fn triangle_edge(space: GamutSpace, uv: vec2f) -> f32 {
  let r = to_uv(space.red);
  let g = to_uv(space.green);
  let b = to_uv(space.blue);
  return min(
    segment_distance(r, g, uv),
    min(segment_distance(g, b, uv), segment_distance(b, r, uv)),
  );
}

fn unpack_tint(packed: u32) -> vec3f {
  return vec3f(
    f32((packed >> 16u) & 255u),
    f32((packed >> 8u) & 255u),
    f32(packed & 255u),
  ) / 255.0;
}

// How full a cell is, on a log scale: the cloud spans four orders of magnitude
// between a stray pixel and a flat sky, and a linear scale shows only the sky.
fn cell_density(cell: vec2i) -> f32 {
  if (cell.x < 0 || cell.y < 0 || cell.x >= i32(XY_BINS) || cell.y >= i32(XY_BINS)) {
    return 0.0;
  }
  let count = xyBins[u32(cell.y) * XY_BINS + u32(cell.x)];
  if (count == 0u) { return 0.0; }
  return 0.35 + 0.65 * log(1.0 + f32(count)) / log(1.0 + f32(scale[0]));
}

// The cloud, read between cells so it reads as a cloud rather than as a grid.
fn density_at(uv: vec2f) -> f32 {
  let t = uv * f32(XY_BINS) - vec2f(0.5);
  let base = vec2i(floor(t));
  let f = fract(t);

  return mix(
    mix(cell_density(base), cell_density(base + vec2i(1, 0)), f.x),
    mix(cell_density(base + vec2i(0, 1)), cell_density(base + vec2i(1, 1)), f.x),
    f.y,
  );
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let px = max(fwidth(in.uv.x), 1e-5);

  var colour = vec3f(0.0);
  var alpha = 0.0;

  // The locus, filled. Softened by a pixel at the boundary, which is a curve
  // approximated by 33 chords and would otherwise show every one of them.
  if (inside_locus(in.uv)) {
    colour = locus_colour(in.uv) * LOCUS_LEVEL;
    alpha = LOCUS_FILL;
  }

  // Tenths on both axes, so a chromaticity can be read off the plot.
  var grid = 0.0;
  for (var i = 1u; i < 9u; i++) {
    let at = f32(i) * 0.1;
    grid = max(grid, stroke(abs(in.uv.x - at / XY_X_MAX), 0.6, px));
    grid = max(grid, stroke(abs(in.uv.y - at / XY_Y_MAX), 0.6, px));
  }
  colour = mix(colour, GRID_TINT, grid * 0.35);
  alpha = max(alpha, grid * 0.25);

  // The image itself.
  let density = density_at(in.uv);
  colour = mix(colour, DATA_TINT, density);
  alpha = max(alpha, density);

  // The gamut triangles, over everything: they are what the panel is asking
  // about, so nothing is allowed to obscure them.
  for (var s = 0u; s < GAMUT_SPACES; s++) {
    let space = overlay.spaces[s];
    if (space.enabled == 0u) { continue; }

    let line = stroke(triangle_edge(space, in.uv), 1.4, px);
    let tint = unpack_tint(space.tint);
    colour = mix(colour, tint, line);
    alpha = max(alpha, line);
  }

  // D65, shared by every space here and so the neutral of all of them.
  let white = stroke(length(in.uv - to_uv(vec2f(0.3127, 0.3290))), 3.0, px);
  colour = mix(colour, vec3f(1.0), white);
  alpha = max(alpha, white);

  return vec4f(colour * alpha, alpha);
}
