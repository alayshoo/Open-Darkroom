// src/lib/gpu/shaders/colorSpace.wgsl
//
// Where the edited image sits in colour, binned two ways in one pass over the
// overview: its chromaticities on the CIE 1931 diagram, and its lightness
// against its hue.
//
// Both are measured in the working space, after the adjustment chain and before
// the output transform. That is the one place the question can be answered: the
// transform fits the picture into whatever is being displayed, so a gamut plot
// taken after it would show every image comfortably inside the display and
// never the colour the edit is asking for.

@group(0) @binding(0) var inputTexture: texture_2d<f32>;

//!include "chunks/params.wgsl"
@group(0) @binding(1) var<storage, read> params: Params;

//!include "chunks/luma.wgsl"
//!include "chunks/tone.wgsl"
//!include "chunks/developChain.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/gamutGrids.wgsl"

@group(0) @binding(2) var<storage, read_write> xyBins: array<atomic<u32>, XY_TOTAL>;
@group(0) @binding(3) var<storage, read_write> hlBins: array<atomic<u32>, HL_TOTAL>;
@group(0) @binding(4) var<storage, read_write> stats:  array<atomic<u32>, STAT_WORDS>;
@group(0) @binding(5) var<storage, read>      overlay: GamutOverlay;

// The grids are too large for workgroup memory — 28k words against a 16KiB
// budget — so they take the global atomics directly. They can afford to: a
// photograph spreads over thousands of cells, where the statistics below are
// five addresses every single pixel votes on.
var<workgroup> local_stats: array<atomic<u32>, STAT_WORDS>;

const WORKGROUP_THREADS: u32 = 256u;

fn bin_chromaticity(xy: vec2f) {
  let cell = vec2u(
    u32(clamp(xy.x / XY_X_MAX, 0.0, 0.9999) * f32(XY_BINS)),
    u32(clamp(xy.y / XY_Y_MAX, 0.0, 0.9999) * f32(XY_BINS)),
  );
  atomicAdd(&xyBins[cell.y * XY_BINS + cell.x], 1u);
}

fn bin_volume(lch: vec3f) {
  let hue = u32(clamp(lch.z, 0.0, 0.9999) * f32(HUE_BINS));
  let light = u32(clamp(lch.x / LIGHT_MAX, 0.0, 0.9999) * f32(LIGHT_BINS));
  let base = (light * HUE_BINS + hue) * 2u;
  atomicAdd(&hlBins[base + HL_COUNT], 1u);
  atomicAdd(&hlBins[base + HL_CHROMA], u32(max(lch.y, 0.0) * CHROMA_SCALE));
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
    let working = develop(textureLoad(inputTexture, gid.xy, 0).rgb);
    let xy = chromaticity(working);

    // A black pixel has no hue and no chromaticity, and counting it would put a
    // spike on whichever cell the division happened to land in.
    if (xy.y >= 0.0) {
      bin_chromaticity(xy);
      bin_volume(oklab_to_lch(working_to_oklab(working)));

      atomicAdd(&local_stats[STAT_COUNTED], 1u);
      for (var s = 0u; s < GAMUT_SPACES; s++) {
        let space = overlay.spaces[s];
        if (space.enabled != 0u && !inside_gamut(space, xy)) {
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
