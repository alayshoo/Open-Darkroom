// The two grids the colour-space tools bin into, and the overlay they are drawn
// against. Mirrored by src/lib/gpu/pipelines/colorSpacePipeline.ts and
// src/lib/gpu/gamutOverlay.ts.

// ── The chromaticity grid: CIE 1931 xy ───────────────────────────────────────
// 0.8 × 0.9 is the conventional plot range, so the canvas is drawn 8 : 9.
const XY_BINS:  u32 = 128u;
const XY_TOTAL: u32 = 16384u;
const XY_X_MAX: f32 = 0.8;
const XY_Y_MAX: f32 = 0.9;

// ── The colour volume: a grid through Oklab ──────────────────────────────────
// Lightness up, a and b across. Only occupancy is stored.
const VOL_BINS:   u32 = 48u;
const VOL_TOTAL:  u32 = 110592u;
const VOL_AB_MAX: f32 = 0.4;
const VOL_L_MAX:  f32 = 1.0;

/// Where in the grid an Oklab colour falls, outside [0, 1) when it falls
/// outside the plotted box.
fn volume_cell(lab: vec3f) -> vec3f {
  return vec3f(
    (lab.y + VOL_AB_MAX) / (2.0 * VOL_AB_MAX),
    lab.x / VOL_L_MAX,
    (lab.z + VOL_AB_MAX) / (2.0 * VOL_AB_MAX),
  );
}

/// The Oklab coordinates of a grid position — the inverse of the above.
fn volume_lab(cell: vec3f) -> vec3f {
  return vec3f(
    cell.y * VOL_L_MAX,
    cell.x * 2.0 * VOL_AB_MAX - VOL_AB_MAX,
    cell.z * 2.0 * VOL_AB_MAX - VOL_AB_MAX,
  );
}

// ── Statistics ───────────────────────────────────────────────────────────────
// Pixels counted, then pixels outside each gamut slot.
const STAT_COUNTED: u32 = 0u;
const STAT_OUTSIDE: u32 = 1u;
const STAT_WORDS:   u32 = 8u;

// ── The overlay ──────────────────────────────────────────────────────────────

const GAMUT_SPACES: u32 = 4u;

// The spectral locus, at 10nm from 380nm to 700nm.
const LOCUS_POINTS: u32 = 33u;

struct GamutSpace {
  red:     vec2f,
  green:   vec2f,
  blue:    vec2f,
  // Non-zero while the triangle is drawn and its pixels counted.
  enabled: u32,
  // Triangle colour, 0xRRGGBB.
  tint:    u32,
}

struct GamutOverlay {
  // Linear destination → CIE XYZ, for measuring after the output transform.
  destination_to_xyz: mat3x3<f32>,
  spaces: array<GamutSpace, GAMUT_SPACES>,
  locus:  array<vec2f, LOCUS_POINTS>,
  // Non-zero to measure after the output transform.
  transformed: u32,
  // Non-zero while the colour volume is binned.
  volume: u32,
}

// Signed distance from an edge, in xy units. The sign depends on the winding.
fn edge_distance(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let edge = b - a;
  return (edge.x * (p.y - a.y) - edge.y * (p.x - a.x)) / max(length(edge), 1e-9);
}

// Points within this distance of a gamut boundary count as inside, so colour
// compressed onto the boundary is not reported as outside it.
const GAMUT_TOLERANCE: f32 = 2e-4;

// Whether a chromaticity falls inside a space's triangle, either winding.
fn inside_gamut(space: GamutSpace, xy: vec2f) -> bool {
  let d = vec3f(
    edge_distance(space.red, space.green, xy),
    edge_distance(space.green, space.blue, xy),
    edge_distance(space.blue, space.red, xy),
  );
  return all(d >= vec3f(-GAMUT_TOLERANCE)) || all(d <= vec3f(GAMUT_TOLERANCE));
}
