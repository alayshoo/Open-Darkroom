// src/lib/gpu/shaders/colorSpace.wgsl
//
// Bins the developed overview into its CIE 1931 chromaticities and its Oklab
// colour volume, and counts the pixels outside each enabled gamut. Measures
// before the output transform, or after it when `overlay.transformed` is set.

@group(0) @binding(0) var inputTexture: texture_2d<f32>;

//!include "chunks/params.wgsl"
@group(0) @binding(1) var<storage, read> params: Params;

//!include "chunks/luma.wgsl"
//!include "chunks/tone.wgsl"
//!include "chunks/developChain.wgsl"
//!include "chunks/outputTransform.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(2) var<storage, read_write> xyBins:  array<atomic<u32>, XY_TOTAL>;
@group(0) @binding(3) var<storage, read_write> volBins: array<atomic<u32>, VOL_TOTAL>;
@group(0) @binding(4) var<storage, read_write> stats:   array<atomic<u32>, STAT_WORDS>;
@group(0) @binding(5) var<storage, read>      overlay: GamutOverlay;

// The preview's output transform, soft proofing included.
@group(0) @binding(6) var<uniform> outputSpace: OutputSpace;

// The grids are too large for workgroup memory; only the statistics are
// accumulated locally.
var<workgroup> local_stats: array<atomic<u32>, STAT_WORDS>;

const WORKGROUP_THREADS: u32 = 256u;

// One pixel through the chain, in XYZ, up to where the panel measures it.
fn measured_xyz(coord: vec2u) -> vec3f {
  let working = develop(textureLoad(inputTexture, coord, 0).rgb);
  if (overlay.transformed == 0u) {
    return M_WORKING_TO_XYZ * working;
  }
  // Linear, before the transfer curve.
  let destination = to_destination_linear(working, outputSpace);
  return overlay.destination_to_xyz * max(destination, vec3f(0.0));
}

fn bin_chromaticity(xy: vec2f) {
  let cell = vec2u(
    u32(clamp(xy.x / XY_X_MAX, 0.0, 0.9999) * f32(XY_BINS)),
    u32(clamp(xy.y / XY_Y_MAX, 0.0, 0.9999) * f32(XY_BINS)),
  );
  atomicAdd(&xyBins[cell.y * XY_BINS + cell.x], 1u);
}

// Colours outside the plotted box are dropped rather than clamped onto its face.
fn bin_volume(lab: vec3f) {
  let cell = volume_cell(lab);
  if (any(cell < vec3f(0.0)) || any(cell >= vec3f(1.0))) { return; }

  let at = vec3u(cell * f32(VOL_BINS));
  atomicAdd(&volBins[(at.z * VOL_BINS + at.y) * VOL_BINS + at.x], 1u);
}

@compute @workgroup_size(16, 16)
fn main(
  @builtin(global_invocation_id) gid: vec3u,
  @builtin(local_invocation_index) lid: u32,
) {
  if (lid < STAT_WORDS) {
    atomicStore(&local_stats[lid], 0u);
  }
  workgroupBarrier();

  // No early return: every invocation must reach both barriers.
  let dimensions = textureDimensions(inputTexture);
  if (gid.x < dimensions.x && gid.y < dimensions.y) {
    let xyz = measured_xyz(gid.xy);
    let sum = xyz.x + xyz.y + xyz.z;

    // Black has no chromaticity.
    if (sum >= COLORIMETRY_EPSILON) {
      bin_chromaticity(vec2f(xyz.x, xyz.y) / sum);
      if (overlay.volume != 0u) {
        bin_volume(xyz_to_oklab(xyz));
      }

      atomicAdd(&local_stats[STAT_COUNTED], 1u);
      for (var s = 0u; s < GAMUT_SPACES; s++) {
        let space = overlay.spaces[s];
        if (space.enabled != 0u && !inside_gamut(space, vec2f(xyz.x, xyz.y) / sum)) {
          atomicAdd(&local_stats[STAT_OUTSIDE + s], 1u);
        }
      }
    }
  }

  workgroupBarrier();

  if (lid < STAT_WORDS) {
    let total = atomicLoad(&local_stats[lid]);
    if (total != 0u) {
      atomicAdd(&stats[lid], total);
    }
  }
}
