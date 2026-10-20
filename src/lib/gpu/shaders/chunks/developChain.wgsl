// Steps 1-12 of the adjustment chain: source channels in, unclamped linear
// working space out. Shared by develop.wgsl, histogram.wgsl and colorSpace.wgsl.
// Requires a `params` binding, chunks/luma.wgsl, chunks/tone.wgsl and
// chunks/colorimetry.wgsl.

// Linear Rec.2020 to and from OKLab's LMS, normalised so that white maps to
// exactly (1, 1, 1), which keeps neutrals at zero chroma.
const M_WORKING_TO_LMS = mat3x3<f32>(
  vec3f(0.6167558, 0.2651330, 0.1001026),
  vec3f(0.3601984, 0.6358394, 0.2039065),
  vec3f(0.0230458, 0.0990276, 0.6959909),
);
const M_LMS_TO_WORKING = mat3x3<f32>(
  vec3f( 2.1399068, -0.8847358, -0.0485737),
  vec3f(-1.2463895,  2.1632308, -0.4545031),
  vec3f( 0.1064827, -0.2784950,  1.5030769),
);

// Saturation (chroma over lightness) below which a pixel's hue is too unstable
// to assign to a band. The colour adjustments fade in across it, so grain in the
// neutrals is left alone. Lightness is floored so near-black noise stays out.
const HSL_NEUTRAL_SAT: f32 = 0.06;
const HSL_SAT_MIN_L: f32 = 0.2;

// Hue shift, chroma scale, lightness scale.
const HSL_NEUTRAL = vec3f(0.0, 1.0, 1.0);

// The bands' adjustments at hue `h` (degrees). Each band crossfades into its
// neighbours between their centres, so the weights always sum to one.
fn hsl_band_adjust(h: f32) -> vec3f {
  var lo = 7u;
  for (var i = 0u; i < 8u; i++) {
    if (h >= params.hsl_bands[i].w) {
      lo = i;
    }
  }
  let band_lo = params.hsl_bands[lo];
  let band_hi = params.hsl_bands[(lo + 1u) % 8u];
  let span = (band_hi.w - band_lo.w + 360.0) % 360.0;
  let t = smoothstep(0.0, 1.0, ((h - band_lo.w + 360.0) % 360.0) / span);
  return mix(band_lo.xyz, band_hi.xyz, t);
}

// Per-colour adjustment in OKLCh: hue rotates h, saturation scales C and
// luminance scales L, each independently of the others.
fn hsl_oklch(rgb: vec3f) -> vec3f {
  let lab = M_LMS_TO_LAB * cbrt_signed(M_WORKING_TO_LMS * rgb);
  let chroma = length(lab.yz);
  let fade = smoothstep(0.0, HSL_NEUTRAL_SAT, chroma / max(lab.x, HSL_SAT_MIN_L));
  if (fade <= 0.0) {
    return rgb;
  }
  let hue = (degrees(atan2(lab.z, lab.y)) + 360.0) % 360.0;
  let adj = mix(HSL_NEUTRAL, hsl_band_adjust(hue), fade);

  let h = radians(hue + adj.x);
  let c = chroma * adj.y;
  let lms = M_LAB_TO_LMS * vec3f(lab.x * adj.z, c * cos(h), c * sin(h));
  return M_LMS_TO_WORKING * (lms * lms * lms);
}

const CURVE_LUT_LAST: f32 = 255.0;

// One channel of the baked point curves, linear between table entries. Past
// either end the curve carries on along its end slope, so values outside 0..1
// are reshaped rather than clipped.
fn curve_channel(x: f32, ch: u32) -> f32 {
  let t = x * CURVE_LUT_LAST;
  if (t <= 0.0) {
    let y0 = params.curve_lut[0][ch];
    return y0 + t * (params.curve_lut[1][ch] - y0);
  }
  if (t >= CURVE_LUT_LAST) {
    let y1 = params.curve_lut[255][ch];
    return y1 + (t - CURVE_LUT_LAST) * (y1 - params.curve_lut[254][ch]);
  }
  let i = u32(t);
  return mix(params.curve_lut[i][ch], params.curve_lut[i + 1u][ch], t - f32(i));
}

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

  // 8b. Point curves — each colour curve, then the master, baked into one table
  //     per channel on the CPU. Skipped while every curve is the identity.
  if (params.curves_active > 0.5) {
    tone = vec3f(
      curve_channel(tone.r, 0u),
      curve_channel(tone.g, 1u),
      curve_channel(tone.b, 2u),
    );
  }

  rgb = decode_srgb(tone);

  // 9. Hue + Saturation (pre-composed matrix from CPU)
  rgb = (params.hueSat_matrix * vec4f(rgb, 1.0)).xyz;

  // 10. Per-colour hue / saturation / luminance, skipped while every band is neutral
  if (params.hsl_active > 0.5) {
    rgb = hsl_oklch(rgb);
  }

  // 11. Vibrance — boost under-saturated pixels more
  //
  // The whole step runs on the colour floored at black, because the saturation
  // it measures — the HSV one, (max - min) / max — has no meaning below zero.
  // Steps 6-8 put colours there routinely: a negative `blacks` or `shadows`, or
  // a cold white balance. Unfloored, the divisor crosses zero and takes the mix
  // factor to infinity with it, and vibrance hauls a below-black channel back
  // into view as bright speckle — a dark ramp reading 0,0,0,1,3 without
  // vibrance came back 2,6,24,0,0 with it.
  //
  // Flooring here is free: the output transform clamps to the same floor.
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

  // 12. Highlight roll-off — compresses everything above the shoulder into 0..1.
  // Luminance mode scales the pixel and keeps its hue; per-channel mode
  // shoulders each channel, which desaturates bright colour but shifts hue.
  if (params.rolloff_mode < 0.5) {
    let luma_ro = dot(rgb, LUMA);
    if (luma_ro > params.rolloff_start) {
      rgb *= shoulder(luma_ro, params.rolloff_start) / luma_ro;
    }
  } else {
    rgb = vec3f(
      shoulder(rgb.r, params.rolloff_start),
      shoulder(rgb.g, params.rolloff_start),
      shoulder(rgb.b, params.rolloff_start),
    );
  }

  return rgb;
}
