// src/lib/gpu/shaders/histogram_compute.wgsl


@group(0) @binding(0) var inputTexture: texture_2d<f32>;        // image input

// Must match the Params struct in develop.wgsl
struct Params {
  dr_matrix:      mat4x4<f32>,  //   0 — 64 bytes
  red_gamma:      f32,          //  64
  green_gamma:    f32,          //  68
  blue_gamma:     f32,          //  72
  out_black:      f32,          //  76
  out_scale:      f32,          //  80
  _pad1:          f32,          //  84
  _pad2:          f32,          //  88
  _pad3:          f32,          //  92  (align wb_matrix to 16)
  wb_matrix:      mat3x3<f32>,  //  96 — 48 bytes (3 cols × 16)
  exposure:       f32,          // 144
  contrast:       f32,          // 148
  brightness:     f32,          // 152
  highlights:     f32,          // 156
  shadows:        f32,          // 160
  whites:         f32,          // 164
  blacks:         f32,          // 168
  _pad4:          f32,          // 172  (align hueSat_matrix to 16)
  hueSat_matrix:  mat4x4<f32>,  // 176 — 64 bytes
  vibrance:       f32,          // 240
  _pad5:          f32,          // 244
  _pad6:          f32,          // 248
  _pad7:          f32,          // 252
}                               // total: 256 bytes
@group(0) @binding(1) var<storage, read> params: Params;

const BINS_PER_CHANNEL: u32 = 256u;
const BIN_TOTAL:        u32 = 768u;   // three channels, laid out end to end
const R_BASE:           u32 = 0u;
const G_BASE:           u32 = 256u;
const B_BASE:           u32 = 512u;

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

// Total weight each pixel contributes, split between the two bins it falls
// between. This also bounds how many pixels a single bin can hold before the
// u32 wraps: u32::MAX / 256 = 16.7M pixels, comfortably above the 4.2M cap of
// a 2048×2048 preview. Raising it buys sub-bin precision at the cost of that
// headroom — at 65536 a bin wrapped once it held just 2.3% of the image.
const VOTE: u32 = 256u;

// ===================== Shared tone maths — must match histogram.wgsl =====================

// BT.709 luminance coefficients
const LUMA = vec3f(0.2126, 0.7152, 0.0722);

// Mid-grey in the encoded domain. Contrast pivots here, so this is the one tone
// that does not move when the contrast slider does.
const TONE_PIVOT: f32 = 0.5;

// Midtone slope at contrast +100. The one free parameter in the curve below.
const CONTRAST_BASE: f32 = 3.0;

// sRGB transfer functions. Values outside 0..1 are passed through the linear
// segment rather than clamped: the chain carries highlight headroom and the
// occasional out-of-gamut negative all the way to the final clamp, and losing
// either here would clip detail the later steps can still recover.
fn encode_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(max(c, vec3f(0.0)), vec3f(1.0 / 2.4)) - 0.055;
  return select(hi, lo, c < vec3f(0.0031308));
}

fn decode_srgb(c: vec3f) -> vec3f {
  let lo = c / 12.92;
  let hi = pow(max((c + 0.055) / 1.055, vec3f(0.0)), vec3f(2.4));
  return select(hi, lo, c < vec3f(0.04045));
}

// Contrast as a power curve on each side of the pivot.
//
//   k > 1  steepens the midtones  (positive contrast)
//   k < 1  flattens them          (negative contrast)
//
// Every property that matters follows from the shape. Both branches fix 0, the
// pivot and 1, so mid-grey holds and the endpoints survive every slider
// position — contrast +100 still reaches true black and true white. The two
// branches meet with slope k on both sides, so there is no seam at the pivot.
// It is monotone for every k > 0, so no setting can double the tone curve back
// on itself. And since k = base^contrast, contrast -50 is the exact inverse of
// contrast +50.
//
// This replaced `mix(rgb, sigmoid(rgb), contrast)`, which failed all four: it
// pivoted on linear 0.5 (sRGB 188, so adding contrast darkened most of the
// frame), it could not reach black or white at +100 (0 → 19, 255 → 254), and
// below contrast -67 it ran backwards, folding smooth gradients.
fn contrast_curve(x: f32, k: f32) -> f32 {
  // Outside the display range the curve is undefined (a power of a negative
  // number); those tones are headroom and are clamped at the end anyway.
  if (x <= 0.0 || x >= 1.0) { return x; }

  if (x < TONE_PIVOT) {
    return TONE_PIVOT * pow(x / TONE_PIVOT, k);
  }
  return 1.0 - (1.0 - TONE_PIVOT) * pow((1.0 - x) / (1.0 - TONE_PIVOT), k);
}
// ========================== (update both files together) ==========================


// One pixel through the same chain develop.wgsl runs, ending in the encoded
// domain the bins measure.
fn developed_srgb(coord: vec2u) -> vec3f {
  var rgb = textureLoad(inputTexture, coord, 0).rgb;

  // ===================== Apply same adjustment chain as develop.wgsl =====================

  // 1. Darkroom matrix — black/white point remap + optional inversion,
  //    normalised so the black point lands on 0 and the white point on 1
  rgb = (params.dr_matrix * vec4f(rgb, 1.0)).xyz;

  // 2. Per-channel gamma
  rgb = vec3f(
    pow(max(rgb.r, 0.0), 1.0 / params.red_gamma),
    pow(max(rgb.g, 0.0), 1.0 / params.green_gamma),
    pow(max(rgb.b, 0.0), 1.0 / params.blue_gamma),
  );

  // 3. Output black/white — the print end of the darkroom block.
  //
  // This sits after gamma, not inside the matrix in step 1, so the two output
  // sliders stay the true endpoints of the result. Applied before the power
  // they were not: output black 80 with gamma 2.2 put pure black at sRGB 153,
  // and output white 200 put pure white at 228. Input remap → gamma → output
  // remap is the order a Levels dialog and a scanner both use.
  rgb = rgb * params.out_scale + params.out_black;

  // 4. White balance (Bradford CAT, pre-computed on CPU)
  rgb = params.wb_matrix * rgb;

  // 5. Exposure — a stop is a multiplication of light, so it stays linear
  rgb *= pow(2.0, params.exposure);

  // ── Steps 6-8 run on a perceptually encoded signal ─────────────────────────
  // Contrast and the four region controls are judgements about how a print
  // looks, not about how much light reached the film, and Lightroom's Basic
  // panel works the same way. Two things depend on it. Contrast pivots on real
  // mid-grey, where linear 0.5 would be sRGB 188 — well up into the highlights.
  // And the mask thresholds below land where their names claim: in linear light
  // 0.5 and 0.75 are sRGB 188 and 225, so "highlights" and "whites" would both
  // be crowded into the top tenth of the visible range while "shadows" covered
  // almost everything else.
  var tone = encode_srgb(rgb);

  // 6. Contrast — symmetric power curve pivoting on mid-grey
  let k = pow(CONTRAST_BASE, params.contrast);
  tone = vec3f(
    contrast_curve(tone.r, k),
    contrast_curve(tone.g, k),
    contrast_curve(tone.b, k),
  );

  // 7. Brightness — a uniform shift in code values
  tone += params.brightness;

  // 8. Highlights / Shadows / Whites / Blacks
  //
  // Each is an offset gated by a luminance mask. The amplitudes arriving here
  // are already budgeted by calcParams so that the sum of their mask slopes can
  // never exceed 1 — see MAX_WIDE_REGION there. Without that budget the tone
  // curve doubles back and shadow detail folds over on itself.
  let luma_tone = dot(tone, LUMA);

  let highlightMask = smoothstep(0.5, 1.0, luma_tone);
  let shadowMask    = 1.0 - smoothstep(0.0, 0.5, luma_tone);
  let whiteMask     = smoothstep(0.75, 1.0, luma_tone);
  let blackMask     = 1.0 - smoothstep(0.0, 0.25, luma_tone);

  tone += params.highlights * highlightMask;
  tone += params.shadows    * shadowMask;
  tone += params.whites     * whiteMask;
  tone += params.blacks     * blackMask;

  rgb = decode_srgb(tone);

  // 9. Hue + Saturation (pre-composed matrix from CPU)
  rgb = (params.hueSat_matrix * vec4f(rgb, 1.0)).xyz;

  // 10. Vibrance — boost under-saturated pixels more
  //
  // The whole step runs on the colour floored at black, because the saturation
  // it measures — the HSV one, (max - min) / max — has no meaning below zero.
  // Steps 6-8 put colours there routinely: a negative `blacks` or `shadows`, or
  // a cold white balance. Unfloored, the divisor crosses zero and takes the mix
  // factor to infinity with it, and vibrance hauls a below-black channel back
  // into view as bright speckle — a dark ramp reading 0,0,0,1,3 without
  // vibrance came back 2,6,24,0,0 with it.
  //
  // Flooring here is free: step 11 clamps to the same floor one line later.
  // Headroom above white is untouched, which is the half the clamp still needs.
  // Flooring only the measurement is not enough — the ratio then jumps from 0
  // to 1 the instant one channel crosses zero while the others are still under
  // it, so a boost that was full at one input is gone at the next.
  //
  // The divisor is floored rather than offset for the same reason of scale: an
  // additive epsilon is only negligible beside a large max, and in linear light
  // the shadows are not large. At sRGB 2 the old `+ 0.001` under-read
  // saturation enough to put a 1.62x boost on an already-saturated pixel.
  let vib_rgb = max(rgb, vec3f(0.0));
  let luma_vib = dot(vib_rgb, LUMA);
  let maxC = max(vib_rgb.r, max(vib_rgb.g, vib_rgb.b));
  let minC = min(vib_rgb.r, min(vib_rgb.g, vib_rgb.b));
  let pixelSat = (maxC - minC) / max(maxC, 1e-4);
  let vibranceAmount = params.vibrance * (1.0 - pixelSat);
  rgb = mix(vec3f(luma_vib), vib_rgb, 1.0 + vibranceAmount);

  // Clamp + linear → sRGB
  // ============================= (update if it ever changes) =============================
  return encode_srgb(clamp(rgb, vec3f(0.0), vec3f(1.0)));
}

// Split one pixel's vote between the two bins its value falls between, per
// channel, into this workgroup's tallies.
fn accumulate(srgb: vec3f) {
  // Fractional bin position in [0, 255]
  let f = clamp(srgb, vec3f(0.0), vec3f(1.0)) * 255.0;

  // Lower bin indices, and upper ones clamped so bin 255 absorbs the right edge
  let lo = vec3u(f);
  let hi = min(lo + vec3u(1u), vec3u(BINS_PER_CHANNEL - 1u));

  // Fractional weight for the upper bin, scaled to [0, VOTE) so each pixel's
  // vote can be split across two adjacent bins without floating-point atomics.
  // Eliminates 8-bit quantisation combing.
  let w = vec3u((f - vec3f(lo)) * f32(VOTE));

  atomicAdd(&local_bins[R_BASE + lo.r], VOTE - w.r);
  atomicAdd(&local_bins[R_BASE + hi.r], w.r);
  atomicAdd(&local_bins[G_BASE + lo.g], VOTE - w.g);
  atomicAdd(&local_bins[G_BASE + hi.g], w.g);
  atomicAdd(&local_bins[B_BASE + lo.b], VOTE - w.b);
  atomicAdd(&local_bins[B_BASE + hi.b], w.b);
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
  }
  workgroupBarrier();

  // Both barriers have to be reached by every invocation in the workgroup, so
  // the edge of the image gates the work rather than returning out of it.
  let dimensions = textureDimensions(inputTexture);
  if (gid.x < dimensions.x && gid.y < dimensions.y) {
    accumulate(developed_srgb(gid.xy));
  }

  workgroupBarrier();

  // Bins no pixel in this workgroup reached are the common case once the image
  // is anything but a full-range gradient, and they are worth not paying for.
  for (var i = lid; i < BIN_TOTAL; i += WORKGROUP_THREADS) {
    let total = atomicLoad(&local_bins[i]);
    if (total != 0u) {
      atomicAdd(&bins[i], total);
    }
  }
}