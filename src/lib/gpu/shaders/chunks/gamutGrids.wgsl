// The two grids the colour-space tools bin into, and the overlay they are drawn
// against. Shared by colorSpace.wgsl, colorSpaceScale.wgsl and the two draws,
// and mirrored by src/lib/gpu/pipelines/colorSpacePipeline.ts.

// ── The chromaticity grid: CIE 1931 xy, one cell per bin ──────────────────────
// The axis ranges are the conventional ones for the diagram, which is why the
// canvas is drawn 0.8 : 0.9 — the two axes then carry the same scale.
const XY_BINS:  u32 = 128u;
const XY_TOTAL: u32 = 16384u;
const XY_X_MAX: f32 = 0.8;
const XY_Y_MAX: f32 = 0.9;

// ── The colour volume: a grid through Oklab ──────────────────────────────────
// Lightness up, a and b across — the axes a colour solid is traditionally
// plotted on. Only occupancy is stored: the colour of a cell is the colour of
// its own coordinates, so there is nothing to average.
const VOL_BINS:   u32 = 48u;
const VOL_TOTAL:  u32 = 110592u;
const VOL_AB_MAX: f32 = 0.4;
const VOL_L_MAX:  f32 = 1.0;

/// Where in the grid an Oklab colour falls, or a negative component when it
/// falls outside the plotted box.
fn volume_cell(lab: vec3f) -> vec3f {
  return vec3f(
    (lab.y + VOL_AB_MAX) / (2.0 * VOL_AB_MAX),
    lab.x / VOL_L_MAX,
    (lab.z + VOL_AB_MAX) / (2.0 * VOL_AB_MAX),
  );
}

/// The Oklab coordinates at the centre of a cell — the inverse of the above.
fn volume_lab(cell: vec3f) -> vec3f {
  return vec3f(
    cell.y * VOL_L_MAX,
    cell.x * 2.0 * VOL_AB_MAX - VOL_AB_MAX,
    cell.z * 2.0 * VOL_AB_MAX - VOL_AB_MAX,
  );
}

// ── Statistics ───────────────────────────────────────────────────────────────
// Tallied per pixel and read back on the CPU, which is what the panel's
// out-of-gamut percentages are.
const STAT_COUNTED: u32 = 0u;
const STAT_OUTSIDE: u32 = 1u;
const STAT_WORDS:   u32 = 8u;

// ── The overlay ──────────────────────────────────────────────────────────────
// Built on the CPU from the published chromaticities, so the triangles the
// diagram draws and the gamut the statistics test against are one description.

const GAMUT_SPACES: u32 = 4u;

// The spectral locus, at 10nm from 380nm to 700nm. The last point joins the
// first across the line of purples.
const LOCUS_POINTS: u32 = 33u;

struct GamutSpace {
  red:     vec2f,
  green:   vec2f,
  blue:    vec2f,
  // Non-zero while the panel is showing this space's triangle. The statistics
  // follow it: a space nobody is looking at is not worth the per-pixel test.
  enabled: u32,
  // The colour the triangle is drawn in, 0xRRGGBB. Carried here so the panel's
  // swatches and the diagram cannot drift apart.
  tint:    u32,
}

struct GamutOverlay {
  // Linear destination → CIE XYZ, for the measurement taken after the output
  // transform. The destination follows the preview, so this cannot be a
  // constant the way the working space's matrix is.
  destination_to_xyz: mat3x3<f32>,
  spaces: array<GamutSpace, GAMUT_SPACES>,
  locus:  array<vec2f, LOCUS_POINTS>,
  // Non-zero while the panel is measuring what the output transform produces
  // rather than what the adjustments asked for.
  transformed: u32,
  // Non-zero while the colour volume is switched on. Nothing else in the panel
  // depends on its grid, so a pixel need not vote into it.
  volume: u32,
}

// How far a point lies to one side of an edge, in the diagram's own units.
// Positive for all three edges of a triangle wound one way and negative for the
// other, so the signs agreeing is what decides containment without anything
// having to know the winding.
fn edge_distance(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let edge = b - a;
  return (edge.x * (p.y - a.y) - edge.y * (p.x - a.x)) / max(length(edge), 1e-9);
}

// A colour sitting exactly on the boundary counts as inside.
//
// Not a nicety: gamut compression works by moving a colour onto the boundary
// and leaving it there, so measured after the output transform a good fraction
// of the image lands on the edge of the destination's triangle to within
// floating-point noise. Without this, "0% outside sRGB" — the whole point of
// the measurement — reads as a stubborn fraction of a percent instead. The
// width is a five-thousandth of the diagram, far below a pixel of it.
const GAMUT_TOLERANCE: f32 = 2e-4;

// Whether a chromaticity falls inside a space's triangle. This asks about hue
// and saturation only — a triangle in xy says nothing about how bright a colour
// may be — which is exactly what the diagram shows.
fn inside_gamut(space: GamutSpace, xy: vec2f) -> bool {
  let d = vec3f(
    edge_distance(space.red, space.green, xy),
    edge_distance(space.green, space.blue, xy),
    edge_distance(space.blue, space.red, xy),
  );
  return all(d >= vec3f(-GAMUT_TOLERANCE)) || all(d <= vec3f(GAMUT_TOLERANCE));
}
