// src/lib/gpu/shaders/colorSpace.wgsl
//
// Where the edited image sits in colour, binned two ways in one pass over the
// overview: its chromaticities on the CIE 1931 diagram, and its colour volume
// through Oklab.
//
// Both can be measured on either side of the output transform, and the two
// answer different questions. Before it is the colour the edit is asking for,
// which is the only place the gamut question can be put: the transform fits the
// picture into whatever is being displayed, so a plot taken after it shows every
// image comfortably inside the display and never what it cost to put it there.
// After it is what the screen is actually showing, and the difference between
// the two is the compression doing its work.

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

// Where the preview is being rendered to, so that the measurement taken after
// the transform is the one the screen is showing — soft proofing included.
@group(0) @binding(6) var<uniform> outputSpace: OutputSpace;

// The grids are far too large for workgroup memory, so they take the global
// atomics directly. They can afford to: a photograph spreads over thousands of
// cells, where the statistics below are five addresses every pixel votes on.
var<workgroup> local_stats: array<atomic<u32>, STAT_WORDS>;

const WORKGROUP_THREADS: u32 = 256u;

// One pixel through the chain, as far as the panel is measuring it.
fn measured_xyz(coord: vec2u) -> vec3f {
  let working = develop(textureLoad(inputTexture, coord, 0).rgb);
  if (overlay.transformed == 0u) {
    return M_WORKING_TO_XYZ * max(working, vec3f(0.0));
  }
  // Stopping short of the transfer curve: that is an encoding, and this is a
  // question about colour.
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

// Colours outside the plotted box are dropped rather than clamped onto its
// face, which would build a wall of colour the image does not contain.
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

  // Both barriers have to be reached by every invocation, so the edge of the
  // image gates the work rather than returning out of it.
  let dimensions = textureDimensions(inputTexture);
  if (gid.x < dimensions.x && gid.y < dimensions.y) {
    let xyz = measured_xyz(gid.xy);
    let sum = xyz.x + xyz.y + xyz.z;

    // A black pixel has no chromaticity, and counting it would put a spike on
    // whichever cell the division happened to land in.
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
