// src/lib/gpu/shaders/histogramScale.wgsl
//
// The value the draw normalises against: the tallest bin in the histogram.
//
// Bins 0 and 255 are left out. A clipped black or white region puts a spike
// there an order of magnitude above everything else, and normalising to it
// flattens the rest of the curve onto the baseline.
//
// One workgroup reduces all 768 bins, so this is a single dispatch of a single
// group — the whole point is that the answer never leaves the GPU.

const BINS_PER_CHANNEL: u32 = 256u;
const BIN_TOTAL:        u32 = 768u;
const THREADS:          u32 = 256u;

@group(0) @binding(0) var<storage, read>       bins:  array<u32, BIN_TOTAL>;
@group(0) @binding(1) var<storage, read_write> scale: array<u32, 1>;

var<workgroup> partial: array<u32, THREADS>;

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lid: u32) {
  var best = 0u;
  for (var i = lid; i < BIN_TOTAL; i += THREADS) {
    let within = i % BINS_PER_CHANNEL;
    if (within != 0u && within != BINS_PER_CHANNEL - 1u) {
      best = max(best, bins[i]);
    }
  }
  partial[lid] = best;
  workgroupBarrier();

  // The barrier sits outside the branch: every invocation runs the same number
  // of iterations, and all of them have to reach each one.
  for (var stride = THREADS / 2u; stride > 0u; stride >>= 1u) {
    if (lid < stride) {
      partial[lid] = max(partial[lid], partial[lid + stride]);
    }
    workgroupBarrier();
  }

  if (lid == 0u) {
    // Never zero — the draw divides by it.
    scale[0] = max(partial[0], 1u);
  }
}
