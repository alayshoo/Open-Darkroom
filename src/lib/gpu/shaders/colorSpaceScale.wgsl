// src/lib/gpu/shaders/colorSpaceScale.wgsl
//
// The fullest cell of the colour volume, which its draw normalises against.

//!include "chunks/gamutGrids.wgsl"

const THREADS: u32 = 256u;

@group(0) @binding(0) var<storage, read>       volBins: array<u32, VOL_TOTAL>;
@group(0) @binding(1) var<storage, read_write> scale:   array<u32, 1>;

var<workgroup> partial: array<u32, THREADS>;

// A max over `partial`, left in slot 0. Uniform control flow for the barriers.
fn reduce(lid: u32) {
  for (var stride = THREADS / 2u; stride > 0u; stride >>= 1u) {
    workgroupBarrier();
    if (lid < stride) {
      partial[lid] = max(partial[lid], partial[lid + stride]);
    }
  }
  workgroupBarrier();
}

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lid: u32) {
  var best = 0u;
  for (var i = lid; i < VOL_TOTAL; i += THREADS) {
    best = max(best, volBins[i]);
  }
  partial[lid] = best;
  reduce(lid);
  if (lid == 0u) {
    // Never zero — the draw divides by it.
    scale[0] = max(partial[0], 1u);
  }
}
