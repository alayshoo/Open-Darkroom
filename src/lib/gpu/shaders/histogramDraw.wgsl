// src/lib/gpu/shaders/histogramDraw.wgsl
//
// The three channel curves in one pass, summed where they overlap so red over
// green reads yellow and all three read white — the mix the 2D canvas made with
// a "lighter" composite.
//
// There is no geometry. Each fragment asks how tall its channel's curve is at
// its own column and whether it falls below that, which is the same filled area
// the polyline described, without a vertex buffer to build per frame.
//
// Output is premultiplied and transparent wherever no channel reaches, so the
// panel's own background shows through exactly as it did behind the 2D canvas.

const BINS_PER_CHANNEL: u32 = 256u;
const BIN_TOTAL:        u32 = 768u;
const R_BASE:           u32 = 0u;
const G_BASE:           u32 = 256u;
const B_BASE:           u32 = 512u;

@group(0) @binding(0) var<storage, read> bins:  array<u32, BIN_TOTAL>;
@group(0) @binding(1) var<storage, read> scale: array<u32, 1>;

struct VertexOut {
  @builtin(position) position: vec4f,
  // x across the bins, y up from the baseline.
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

// Height of one channel's curve at `x`, as a fraction of the panel.
//
// Linear between neighbouring bins rather than a step at each one: the polyline
// this replaces joined the bin points with straight segments, and at 256 bins
// across a panel a few hundred pixels wide the difference is visible.
fn curve(base: u32, x: f32) -> f32 {
  let ceiling = f32(scale[0]);
  let t = clamp(x, 0.0, 1.0) * f32(BINS_PER_CHANNEL - 1u);
  let i0 = u32(floor(t));
  let i1 = min(i0 + 1u, BINS_PER_CHANNEL - 1u);

  let lo = min(f32(bins[base + i0]) / ceiling, 1.0);
  let hi = min(f32(bins[base + i1]) / ceiling, 1.0);
  return mix(lo, hi, fract(t));
}

// How much of this fragment the curve covers. The screen-space derivative is
// one pixel in the same units as the curve, which is what keeps the top edge
// from staircasing the way a hard threshold would.
fn coverage(height: f32, value: f32) -> f32 {
  let aa = max(fwidth(height), 1e-5);
  return smoothstep(-aa, aa, value - height);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let height = in.uv.y;
  let colour = vec3f(
    coverage(height, curve(R_BASE, in.uv.x)),
    coverage(height, curve(G_BASE, in.uv.x)),
    coverage(height, curve(B_BASE, in.uv.x)),
  );

  // Already premultiplied: no channel can exceed the alpha, since the alpha is
  // the largest of them.
  return vec4f(colour, max(colour.r, max(colour.g, colour.b)));
}
