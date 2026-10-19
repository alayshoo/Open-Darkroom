// The tone domain the whole chain works in: where contrast pivots, how the
// signal moves between linear and sRGB, and the curve itself.
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

// Linear value the highlight shoulder maps to exactly 1: three stops above white.
const ROLLOFF_WHITE: f32 = 8.0;

// Highlight shoulder: identity up to `start`, then extended Reinhard with
// matching slope at the join, reaching 1 at ROLLOFF_WHITE and holding it above.
fn shoulder(x: f32, start: f32) -> f32 {
  if (x <= start || start >= 1.0) {
    return x;
  }
  let span = 1.0 - start;
  let t = (x - start) / span;
  let t_white = (ROLLOFF_WHITE - start) / span;
  return min(start + span * t * (1.0 + t / (t_white * t_white)) / (1.0 + t), 1.0);
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
