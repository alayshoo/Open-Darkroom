// src/lib/gpu/shaders/histogram_compute.wgsl


@group(0) @binding(0) var inputTexture: texture_2d<f32>;        // image input

//!include "chunks/params.wgsl"
@group(0) @binding(1) var<storage, read> params: Params;

//!include "chunks/histogramBins.wgsl"

// One buffer rather than three: the three are always written together and read
// back together, so splitting them only multiplies the copies and the maps.
@group(0) @binding(2) var<storage, read_write> bins: array<atomic<u32>, BIN_TOTAL>;

// Per-workgroup tallies, merged into `bins` once at the end.
//
// Every pixel voting straight into storage put six atomics per pixel onto 768
// addresses, and a photograph is exactly the contention worst case: a flat sky
// aims millions of them at one bin. Accumulating in workgroup memory first
// leaves one global atomic per bin per workgroup instead.
var<workgroup> local_bins: array<atomic<u32>, BIN_TOTAL>;

// The master input levels' bins, measured on the channels' combined result just
// ahead of the master step. Only written while measuring the darkroom block.
@group(0) @binding(5) var<storage, read_write> master_bins: array<atomic<u32>, BIN_TOTAL>;
var<workgroup> local_master: array<atomic<u32>, BIN_TOTAL>;

// Total weight each pixel contributes, split between the two bins it falls
// between. This also bounds how many pixels a single bin can hold before the
// u32 wraps: u32::MAX / 256 = 16.7M pixels, comfortably above the 4.2M cap of
// a 2048×2048 preview. Raising it buys sub-bin precision at the cost of that
// headroom — at 65536 a bin wrapped once it held just 2.3% of the image.
const VOTE: u32 = 256u;

//!include "chunks/luma.wgsl"
//!include "chunks/tone.wgsl"
//!include "chunks/colorimetry.wgsl"
//!include "chunks/developChain.wgsl"
//!include "chunks/outputTransform.wgsl"

// The destination the bins are measured in, so the curves and the clipping
// warnings follow whatever the preview is currently being rendered for.
@group(0) @binding(3) var<uniform> outputSpace: OutputSpace;

//!include "chunks/cropView.wgsl"

// The crop, mapped onto this texture, so only what will be kept is binned.
@group(0) @binding(4) var<uniform> crop: CropView;

fn in_crop(p: vec2f) -> bool {
  let d = p - crop.origin;
  let u = dot(d, crop.axis_x) / dot(crop.axis_x, crop.axis_x);
  let v = dot(d, crop.axis_y) / dot(crop.axis_y, crop.axis_y);
  return u >= 0.0 && u < crop.size.x && v >= 0.0 && v < crop.size.y;
}

// Set while bracketing: the bins then measure the darkroom block's output in
// the source's own channels, sRGB encoded to match the sliders, so each channel
// moves only with its own sliders. Off, they measure the image on screen.
override MEASURE_DARKROOM: bool = false;

// Fractional bin positions of one pixel, per channel: the lower bin, the upper
// one, and the upper one's share of the vote.
struct Vote {
  lo: vec3u,
  hi: vec3u,
  w:  vec3u,
}

fn vote(encoded: vec3f) -> Vote {
  // Fractional bin position in [0, 255]
  let f = clamp(encoded, vec3f(0.0), vec3f(1.0)) * 255.0;

  // Lower bin indices, and upper ones clamped so bin 255 absorbs the right edge
  let lo = vec3u(f);
  let hi = min(lo + vec3u(1u), vec3u(BINS_PER_CHANNEL - 1u));

  // Fractional weight for the upper bin, scaled to [0, VOTE) so each pixel's
  // vote can be split across two adjacent bins without floating-point atomics.
  // Eliminates 8-bit quantisation combing.
  let w = vec3u((f - vec3f(lo)) * f32(VOTE));
  return Vote(lo, hi, w);
}

// Split one pixel's vote between the two bins its value falls between, per
// channel, into this workgroup's tallies.
fn accumulate(encoded: vec3f) {
  let v = vote(encoded);
  atomicAdd(&local_bins[R_BASE + v.lo.r], VOTE - v.w.r);
  atomicAdd(&local_bins[R_BASE + v.hi.r], v.w.r);
  atomicAdd(&local_bins[G_BASE + v.lo.g], VOTE - v.w.g);
  atomicAdd(&local_bins[G_BASE + v.hi.g], v.w.g);
  atomicAdd(&local_bins[B_BASE + v.lo.b], VOTE - v.w.b);
  atomicAdd(&local_bins[B_BASE + v.hi.b], v.w.b);
}

fn accumulate_master(encoded: vec3f) {
  let v = vote(encoded);
  atomicAdd(&local_master[R_BASE + v.lo.r], VOTE - v.w.r);
  atomicAdd(&local_master[R_BASE + v.hi.r], v.w.r);
  atomicAdd(&local_master[G_BASE + v.lo.g], VOTE - v.w.g);
  atomicAdd(&local_master[G_BASE + v.hi.g], v.w.g);
  atomicAdd(&local_master[B_BASE + v.lo.b], VOTE - v.w.b);
  atomicAdd(&local_master[B_BASE + v.hi.b], v.w.b);
}

// One pixel through the same chain develop.wgsl runs. The image on screen is
// measured in whichever destination the preview is currently rendered for.
fn measure(coord: vec2u) {
  let source = textureLoad(inputTexture, coord, 0).rgb;
  if (MEASURE_DARKROOM) {
    let channels = darkroom_channels(source);
    accumulate(encode_srgb(darkroom_master(channels)));
    accumulate_master(encode_srgb(channels));
    return;
  }
  accumulate(to_destination(develop(source), outputSpace));
}

// 16 × 16, so the strided loops below cover 768 bins three entries at a time.
const WORKGROUP_THREADS: u32 = 256u;

@compute @workgroup_size(16, 16)
fn main(
  @builtin(global_invocation_id) gid: vec3u,
  @builtin(local_invocation_index) lid: u32,
) {
  for (var i = lid; i < BIN_TOTAL; i += WORKGROUP_THREADS) {
    atomicStore(&local_bins[i], 0u);
    if (MEASURE_DARKROOM) {
      atomicStore(&local_master[i], 0u);
    }
  }
  workgroupBarrier();

  // Both barriers have to be reached by every invocation in the workgroup, so
  // the edge of the image gates the work rather than returning out of it.
  let dimensions = textureDimensions(inputTexture);
  if (gid.x < dimensions.x && gid.y < dimensions.y && in_crop(vec2f(gid.xy) + 0.5)) {
    measure(gid.xy);
  }

  workgroupBarrier();

  // Bins no pixel in this workgroup reached are the common case once the image
  // is anything but a full-range gradient, and they are worth not paying for.
  for (var i = lid; i < BIN_TOTAL; i += WORKGROUP_THREADS) {
    let total = atomicLoad(&local_bins[i]);
    if (total != 0u) {
      atomicAdd(&bins[i], total);
    }
    if (MEASURE_DARKROOM) {
      let master_total = atomicLoad(&local_master[i]);
      if (master_total != 0u) {
        atomicAdd(&master_bins[i], master_total);
      }
    }
  }
}