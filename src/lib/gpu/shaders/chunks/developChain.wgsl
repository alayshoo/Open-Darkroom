// Steps 1-10 of the adjustment chain: the file's own channels in, linear
// working space out, nothing clamped.
//
// Three shaders run it — develop.wgsl renders it, histogram.wgsl bins it, and
// colorSpace.wgsl measures its gamut. They have to agree to the bit: a
// histogram or a gamut plot of a different chain describes an image nobody is
// being shown.
//
// Requires a `params` binding of type Params, plus chunks/luma.wgsl for LUMA
// and M_WORKING_TO_SRGB and chunks/tone.wgsl for the sRGB pair and the curve.
//
// The result is deliberately unclamped and still in the working space. What
// each caller does next is its own business: develop encodes and hands off to
// sharpening, the other two carry on into the output transform.
fn develop(source: vec3f) -> vec3f {
  var rgb = source;

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

  // 4. Source → working space, composed with white balance (Bradford CAT).
  //
  // This is the line the darkroom block ends at. Above it the channels are the
  // file's own, which is what lets steps 1-3 address the film's three dye layers
  // separately; below it they are the working space's. calcParams folds the two
  // matrices into one because both are linear and run back to back.
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

  // Saturation is measured in the destination space, not the working one: it
  // asks how colourful the pixel looks, and the working space is wider than
  // anything it will be shown in. Measured in Rec.2020 the vivid colours lose
  // the most, enough to boost a vivid red harder than a muted one. Only the
  // measurement moves; the boost still happens in the working space.
  let seen = max(M_WORKING_TO_SRGB * vib_rgb, vec3f(0.0));
  let maxC = max(seen.r, max(seen.g, seen.b));
  let minC = min(seen.r, min(seen.g, seen.b));
  let pixelSat = (maxC - minC) / max(maxC, 1e-4);
  let vibranceAmount = params.vibrance * (1.0 - pixelSat);
  rgb = mix(vec3f(luma_vib), vib_rgb, 1.0 + vibranceAmount);

  return rgb;
}
