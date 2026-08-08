// src/lib/gpu/shaders/develop.wgsl


// group(0) = bind group index, binding(N) = slot within group.

@group(0) @binding(0) var inputTexture: texture_2d<f32>;  // Image being processed

// A sampler defines how to read the texture (filtering, wrapping, etc.)
@group(0) @binding(1) var texSampler: sampler;

// Uniform buffer = small, read-only data shared across all pixels.
// Params must equate to a multiple of 16 bytes, each f32 is 4 bytes, mat3x3<f32> is 48 bytes
struct Params {
  dr_matrix:      mat4x4<f32>,  //   0 — 64 bytes
  red_gamma:      f32,          //  64
  green_gamma:    f32,          //  68
  blue_gamma:     f32,          //  72
  _pad1:          f32,          //  76  (align wb_matrix to 16)
  wb_matrix:      mat3x3<f32>,  //  80 — 48 bytes (3 cols × 16)
  exposure:       f32,          // 128
  contrast:       f32,          // 132
  brightness:     f32,          // 136
  highlights:     f32,          // 140
  shadows:        f32,          // 144
  whites:         f32,          // 148
  blacks:         f32,          // 152
  _pad2:          f32,          // 156  (align hueSat_matrix to 16)
  hueSat_matrix:  mat4x4<f32>,  // 160 — 64 bytes
  vibrance:       f32,          // 224
  _pad3:          f32,          // 228
  _pad4:          f32,          // 232
  _pad5:          f32,          // 236
}                               // total: 240 bytes
@group(0) @binding(2) var<storage, read> params: Params;

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

// ---------- Vertex Shader ----------
// Runs once per vertex (6 vertices = 2 triangles = full screen quad).
// This is essentially selecting the pixels that we are modifying. It draws two triangles to make a quad.
// Since we are selecting everything from -1.0 to 1.0 on the x axis and -1.0 to 1.0 in the y axis we basically select all pixels.

struct VertexOutput {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> VertexOutput {
  // Hardcoded full-screen triangle trick (2 triangles, 6 vertices)
  // Maps vertex indices 0-5 to positions covering clip space [-1, 1]
  var pos = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f( 1.0, -1.0),
    vec2f(-1.0,  1.0),
    vec2f(-1.0,  1.0),
    vec2f( 1.0, -1.0),
    vec2f( 1.0,  1.0),
  );

  var uv = array<vec2f, 6>(
    vec2f(0.0, 1.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 0.0),
    vec2f(0.0, 0.0),
    vec2f(1.0, 1.0),
    vec2f(1.0, 0.0),
  );

  var output: VertexOutput;
  output.position = vec4f(pos[vertexIndex], 0.0, 1.0);
  output.uv = uv[vertexIndex];
  return output;
}

// ---------- Fragment Shader ----------
// Runs once per slected  pixel. This is where the actual image processing happens.

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {

  let color = textureSample(inputTexture, texSampler, input.uv);
  var rgb = color.rgb;

  // 1. Darkroom matrix — black/white point remap + optional inversion
  rgb = (params.dr_matrix * vec4f(rgb, 1.0)).xyz;

  // 2. Per-channel gamma
  rgb = vec3f(
    pow(max(rgb.r, 0.0), 1.0 / params.red_gamma),
    pow(max(rgb.g, 0.0), 1.0 / params.green_gamma),
    pow(max(rgb.b, 0.0), 1.0 / params.blue_gamma),
  );

  // 3. White balance (Bradford CAT, pre-computed on CPU)
  rgb = params.wb_matrix * rgb;

  // 4. Exposure — a stop is a multiplication of light, so it stays linear
  rgb *= pow(2.0, params.exposure);

  // ── Steps 5-7 run on a perceptually encoded signal ─────────────────────────
  // Contrast and the four region controls are judgements about how a print
  // looks, not about how much light reached the film, and Lightroom's Basic
  // panel works the same way. Two things depend on it. Contrast pivots on real
  // mid-grey, where linear 0.5 would be sRGB 188 — well up into the highlights.
  // And the mask thresholds below land where their names claim: in linear light
  // 0.5 and 0.75 are sRGB 188 and 225, so "highlights" and "whites" would both
  // be crowded into the top tenth of the visible range while "shadows" covered
  // almost everything else.
  var tone = encode_srgb(rgb);

  // 5. Contrast — symmetric power curve pivoting on mid-grey
  let k = pow(CONTRAST_BASE, params.contrast);
  tone = vec3f(
    contrast_curve(tone.r, k),
    contrast_curve(tone.g, k),
    contrast_curve(tone.b, k),
  );

  // 6. Brightness — a uniform shift in code values
  tone += params.brightness;

  // 7. Highlights / Shadows / Whites / Blacks
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

  // 8. Hue + Saturation (pre-composed matrix from CPU)
  rgb = (params.hueSat_matrix * vec4f(rgb, 1.0)).xyz;

  // 9. Vibrance — boost under-saturated pixels more
  let luma_vib = dot(rgb, LUMA);
  let maxC = max(rgb.r, max(rgb.g, rgb.b));
  let minC = min(rgb.r, min(rgb.g, rgb.b));
  let pixelSat = (maxC - minC) / (maxC + 0.001);
  let vibranceAmount = params.vibrance * (1.0 - pixelSat);
  rgb = mix(vec3f(luma_vib), rgb, 1.0 + vibranceAmount);

  // Clamp + linear → sRGB
  let srgb = encode_srgb(clamp(rgb, vec3f(0.0), vec3f(1.0)));

  return vec4f(srgb, color.a);
}