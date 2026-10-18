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

// ── The colour volume grid: hue across, lightness up ─────────────────────────
// Two words per cell, a count and the chroma summed over it, so the plot can
// draw each cell at the chroma its pixels actually reached.
const HUE_BINS:   u32 = 96u;
const LIGHT_BINS: u32 = 64u;
const HL_CELLS:   u32 = 6144u;
const HL_TOTAL:   u32 = 12288u;
const HL_COUNT:   u32 = 0u;
const HL_CHROMA:  u32 = 1u;

// Chroma is summed as an integer, so it is scaled on the way in. 4096 leaves an
// Oklab chroma of 0.4 across a million-pixel overview two bits short of the u32
// ceiling, and resolves 1/4096 — far finer than a cell is drawn.
const CHROMA_SCALE: f32 = 4096.0;

// Lightness above this has nowhere left to go on the plot; Oklab L is 1.0 at
// the working space's white, and the chain carries highlights above it.
const LIGHT_MAX: f32 = 1.0;

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
  spaces: array<GamutSpace, GAMUT_SPACES>,
  locus:  array<vec2f, LOCUS_POINTS>,
}

// Which side of the edge a point falls on. Positive for all three edges of a
// triangle wound one way, negative for the other, so the sign of the product is
// what decides containment without knowing the winding.
fn edge_side(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  return (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
}

// Whether a chromaticity falls inside a space's triangle. This asks about hue
// and saturation only — a triangle in xy says nothing about how bright a colour
// may be — which is exactly what the diagram shows.
fn inside_gamut(space: GamutSpace, xy: vec2f) -> bool {
  let a = edge_side(space.red, space.green, xy);
  let b = edge_side(space.green, space.blue, xy);
  let c = edge_side(space.blue, space.red, xy);
  return (a >= 0.0 && b >= 0.0 && c >= 0.0) || (a <= 0.0 && b <= 0.0 && c <= 0.0);
}
