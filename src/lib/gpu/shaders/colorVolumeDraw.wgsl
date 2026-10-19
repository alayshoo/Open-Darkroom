// src/lib/gpu/shaders/colorVolumeDraw.wgsl
//
// The colour volume: occupied Oklab cells raymarched as a solid inside a
// wireframe box, lightness up and a/b across.

//!include "chunks/tone.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(0) var<storage, read> volBins: array<u32, VOL_TOTAL>;
@group(0) @binding(1) var<storage, read> scale:   array<u32, 2>;

// Where the box is being looked at from.
struct Camera {
  yaw:    f32,
  pitch:  f32,
  // Width over height of the target.
  aspect: f32,
  // Half the box's extent in clip units.
  zoom:   f32,
}

@group(0) @binding(2) var<uniform> camera: Camera;

// Steps along the ray, enough to visit every cell on the box's diagonal.
const STEPS: u32 = 96u;

const BOX_TINT = vec3f(0.68, 0.71, 0.76);
const EDGE_ALPHA: f32 = 0.7;
// Opacity kept by edges the solid hides.
const HIDDEN_EDGE: f32 = 0.3;

struct VertexOut {
  @builtin(position) position: vec4f,
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
  out.uv = p;
  return out;
}

// The camera's basis. Yaw turns about the lightness axis; pitch tilts the eye.
fn basis() -> mat3x3<f32> {
  let cy = cos(camera.yaw);
  let sy = sin(camera.yaw);
  let cp = cos(camera.pitch);
  let sp = sin(camera.pitch);

  let right   = vec3f(cy, 0.0, -sy);
  let up      = vec3f(sy * sp, cp, cy * sp);
  let forward = vec3f(sy * cp, -sp, cy * cp);
  return mat3x3<f32>(right, up, forward);
}

/// A point in the unit box projected to the panel, with its depth measured
/// from the box's centre.
fn project(cell: vec3f, view: mat3x3<f32>) -> vec3f {
  let centred = cell - vec3f(0.5);
  let eye = transpose(view) * centred;
  return vec3f(eye.x / (camera.zoom * camera.aspect), eye.y / camera.zoom, eye.z);
}

/// The slab test: where a ray enters and leaves the unit box.
fn box_span(origin: vec3f, direction: vec3f) -> vec2f {
  let inverse = 1.0 / direction;
  let a = (vec3f(0.0) - origin) * inverse;
  let b = (vec3f(1.0) - origin) * inverse;
  let near = min(a, b);
  let far = max(a, b);
  return vec2f(
    max(near.x, max(near.y, near.z)),
    min(far.x, min(far.y, far.z)),
  );
}

fn occupancy(cell: vec3f) -> u32 {
  if (any(cell < vec3f(0.0)) || any(cell >= vec3f(1.0))) { return 0u; }
  let at = vec3u(cell * f32(VOL_BINS));
  return volBins[(at.z * VOL_BINS + at.y) * VOL_BINS + at.x];
}

/// The colour of a cell's own coordinates, clamped to the display.
fn cell_colour(cell: vec3f) -> vec3f {
  let working = oklab_to_working(volume_lab(cell));
  return encode_srgb(clamp(working, vec3f(0.0), vec3f(1.0)));
}

/// Depth-based shading, so the solid does not read as a flat patch.
fn relief(t: f32, span: vec2f) -> f32 {
  let depth = clamp((t - span.x) / max(span.y - span.x, 1e-5), 0.0, 1.0);
  return mix(1.15, 0.78, depth);
}

/// Straight-alpha source over destination, both carried as (rgb, a).
fn over(src: vec4f, dst: vec4f) -> vec4f {
  let a = src.a + dst.a * (1.0 - src.a);
  if (a < 1e-5) { return vec4f(0.0); }
  return vec4f((src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a, a);
}

fn segment_distance(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let ab = b - a;
  let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-9), 0.0, 1.0);
  return length(p - (a + ab * t));
}

/// Where along a segment the closest point to `p` falls.
fn segment_at(a: vec2f, b: vec2f, p: vec2f) -> f32 {
  let ab = b - a;
  return clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-9), 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4f {
  let px = max(fwidth(in.uv.x), 1e-5);
  let view = basis();

  // Orthographic ray.
  let right = view[0];
  let up = view[1];
  let forward = view[2];
  let origin = vec3f(0.5)
    + right * (in.uv.x * camera.zoom * camera.aspect)
    + up * (in.uv.y * camera.zoom)
    - forward * 2.0;

  let span = box_span(origin, forward);

  var colour = vec3f(0.0);
  var alpha = 0.0;
  // Where along the ray the solid was hit, for hiding the wireframe behind it.
  var hit = 1e9;

  if (span.y > span.x && span.y > 0.0) {
    let start = max(span.x, 0.0);
    let stride = (span.y - start) / f32(STEPS);

    for (var i = 0u; i < STEPS; i++) {
      let t = start + (f32(i) + 0.5) * stride;
      let cell = origin + forward * t;
      let count = occupancy(cell);
      if (count == 0u) { continue; }

      // The first occupied cell is the surface; sparse cells are drawn fainter.
      let density = log(1.0 + f32(count)) / log(1.0 + f32(scale[1]));
      colour = cell_colour(cell) * relief(t, span);
      alpha = clamp(0.35 + 2.2 * density, 0.0, 1.0);
      hit = t;
      break;
    }
  }

  // The box's twelve edges, faded where the solid stands in front of them.
  var front = 0.0;
  var back = 0.0;
  for (var e = 0u; e < 12u; e++) {
    // Four edges per axis.
    let axis = e / 4u;
    let corner = e % 4u;
    let other = vec2f(f32(corner & 1u), f32((corner >> 1u) & 1u));

    var a = vec3f(other.x, other.y, 0.0);
    var b = vec3f(other.x, other.y, 1.0);
    if (axis == 0u) {
      a = vec3f(0.0, other.x, other.y);
      b = vec3f(1.0, other.x, other.y);
    } else if (axis == 1u) {
      a = vec3f(other.x, 0.0, other.y);
      b = vec3f(other.x, 1.0, other.y);
    }

    let pa = project(a, view);
    let pb = project(b, view);
    let line = 1.0 - smoothstep(0.0, px * 1.6, segment_distance(pa.xy, pb.xy, in.uv));
    if (line <= 0.0) { continue; }

    // Projected depth is from the box's centre; the ray starts two units before.
    let depth = mix(pa.z, pb.z, segment_at(pa.xy, pb.xy, in.uv)) + 2.0;
    if (hit < 1e8 && depth > hit) {
      back = max(back, line);
    } else {
      front = max(front, line);
    }
  }

  var layered = over(vec4f(BOX_TINT, back * EDGE_ALPHA * HIDDEN_EDGE), vec4f(0.0));
  layered = over(vec4f(colour, alpha), layered);
  layered = over(vec4f(BOX_TINT, front * EDGE_ALPHA), layered);

  return vec4f(layered.rgb * layered.a, layered.a);
}
