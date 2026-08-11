// src/lib/gpu/shaders/calcSharpTexture.wgsl
//
// Extracts the sharpness detail bands from the developed image. Runs as two
// passes over the same file — horizontal, then vertical.
//
// The blur is separable: blurring rows and then columns gives exactly the same
// result as the full 2D kernel, at a fraction of the cost. At clarity's radius
// that is 361 taps per axis against 130321 for the 2D form, and the gap widens
// with the square of the radius. The price is one intermediate texture.
//
// Three bands come out of four blurs, all of luminance alone. A scalar band per
// scale means each fits in one channel, and it is what keeps sharpening from
// tinting edges — composite.wgsl scales the colour by the change in its luma
// rather than adding to the channels, so hue and saturation come through
// untouched.
//
// Texture and clarity are adjacent band-passes sharing a cutoff, so the scales
// they act on do not overlap. The unsharp mask deliberately does overlap both:
// it is the explicit sharpening control, and its job is the finest detail
// whatever else is in play.
//
// Intermediate channels — one horizontally blurred luma per scale:
//   .r  unsharp mask     .g  texture, low    .b  texture, high    .a  clarity
//
// Output channels — the finished bands:
//   .r  unsharp mask     high-pass at the user's radius
//   .g  texture          band-pass between two fixed scales
//   .b  clarity          band-pass from texture's upper cutoff to its own
//   .a  the blurred luma the unsharp mask's luma gate reads
//
// Each scale is blurred with its own radius rather than all four at the widest.
// Clarity's kernel is an order of magnitude wider than texture's, so sharing one
// would multiply the cost of the three narrow bands by that ratio for nothing.

// Must match SharpParams in calcParams.wgsl and composite.wgsl
struct SharpParams {
    usm_amount:           f32,   //  0
    usm_sigma:            f32,   //  4
    usm_luma_threshold:   f32,   //  8
    usm_detail_threshold: f32,   // 12
    texture_amount:       f32,   // 16
    texture_sigma_lo:     f32,   // 20
    texture_sigma_hi:     f32,   // 24
    clarity_amount:       f32,   // 28
    clarity_sigma:        f32,   // 32
    _pad0:                f32,   // 36
    _pad1:                f32,   // 40
    _pad2:                f32,   // 44
}                                // total: 48 bytes

// The texture this pass blurs: the developed image for the horizontal pass, the
// horizontal pass's own output for the vertical one.
@group(0) @binding(0) var srcTexture: texture_2d<f32>;
// The developed image, bound only by the vertical pass: it needs the unblurred
// luma at its own pixel to subtract, and by then the intermediate holds four
// blurs and has no channel left to have carried it.
@group(0) @binding(1) var developedTexture: texture_2d<f32>;
@group(0) @binding(2) var<storage, read> sharp: SharpParams;

// BT.709 luminance coefficients
const LUMA = vec3f(0.2126, 0.7152, 0.0722);

// A Gaussian is numerically dead past three standard deviations, so that is
// where the kernel is truncated. This bound is what the widest sigma any band
// reaches — clarity's, at 60 full-resolution pixels — asks for. Each loop stops
// at the radius its own sigma needs, and since the sigmas are uniform across the
// draw, every invocation of a given loop takes the same number of steps.
const MAX_RADIUS: i32 = 180;

// One-hot selectors for reading a single channel back out of the intermediate.
const CH_USM         = vec4f(1.0, 0.0, 0.0, 0.0);
const CH_TEXTURE_LO  = vec4f(0.0, 1.0, 0.0, 0.0);
const CH_TEXTURE_HI  = vec4f(0.0, 0.0, 1.0, 0.0);
const CH_CLARITY     = vec4f(0.0, 0.0, 0.0, 1.0);

@vertex
fn vs_main(@builtin(vertex_index) vertexIndex: u32) -> @builtin(position) vec4f {
  // Full-screen quad, 2 triangles, 6 vertices.
  var pos = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f( 1.0, -1.0),
    vec2f(-1.0,  1.0),
    vec2f(-1.0,  1.0),
    vec2f( 1.0, -1.0),
    vec2f( 1.0,  1.0),
  );
  return vec4f(pos[vertexIndex], 0.0, 1.0);
}

fn kernel_radius(sigma: f32) -> i32 {
  return min(i32(ceil(sigma * 3.0)), MAX_RADIUS);
}

// Blur the developed image along x, reducing each tap to luma on the way in.
//
// Taps off the edge of the image are clamped to the edge rather than dropped, so
// the weights always sum to one and the border keeps the brightness of its
// neighbours instead of darkening toward an implied black surround.
fn blur_h(coord: vec2i, sigma_raw: f32) -> f32 {
  let dims = vec2i(textureDimensions(srcTexture));
  let sigma = max(sigma_raw, 1e-4);
  let radius = kernel_radius(sigma);
  let denom = 2.0 * sigma * sigma;

  var total = 0.0;
  var weight_total = 0.0;

  for (var i = -radius; i <= radius; i++) {
    let f = f32(i);
    let w = exp(-(f * f) / denom);
    let at = clamp(coord + vec2i(i, 0), vec2i(0, 0), dims - vec2i(1, 1));
    total += w * dot(textureLoad(srcTexture, at, 0).rgb, LUMA);
    weight_total += w;
  }

  return total / weight_total;
}

// Finish one scale's blur along y, reading the channel `mask` selects.
fn blur_v(coord: vec2i, mask: vec4f, sigma_raw: f32) -> f32 {
  let dims = vec2i(textureDimensions(srcTexture));
  let sigma = max(sigma_raw, 1e-4);
  let radius = kernel_radius(sigma);
  let denom = 2.0 * sigma * sigma;

  var total = 0.0;
  var weight_total = 0.0;

  for (var i = -radius; i <= radius; i++) {
    let f = f32(i);
    let w = exp(-(f * f) / denom);
    let at = clamp(coord + vec2i(0, i), vec2i(0, 0), dims - vec2i(1, 1));
    total += w * dot(textureLoad(srcTexture, at, 0), mask);
    weight_total += w;
  }

  return total / weight_total;
}

@fragment
fn h_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let coord = vec2i(position.xy);
  return vec4f(
    blur_h(coord, sharp.usm_sigma),
    blur_h(coord, sharp.texture_sigma_lo),
    blur_h(coord, sharp.texture_sigma_hi),
    blur_h(coord, sharp.clarity_sigma),
  );
}

@fragment
fn v_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let coord = vec2i(position.xy);
  let luma = dot(textureLoad(developedTexture, coord, 0).rgb, LUMA);

  let usm_blur = blur_v(coord, CH_USM, sharp.usm_sigma);
  let texture_lo = blur_v(coord, CH_TEXTURE_LO, sharp.texture_sigma_lo);
  let texture_hi = blur_v(coord, CH_TEXTURE_HI, sharp.texture_sigma_hi);
  let clarity_blur = blur_v(coord, CH_CLARITY, sharp.clarity_sigma);

  // Texture and clarity meet at texture's upper sigma and share it: it is the
  // top of one band and the bottom of the next. So the two partition the range
  // of scales between them rather than overlapping — pushing both sliders cannot
  // count the same detail twice, and clarity cannot lift grain, which on a film
  // scan is the difference between local contrast and a ruined frame.
  //
  // This is why clarity subtracts from `texture_hi` rather than from the luma.
  // As a plain high-pass it would pass everything finer than its own radius,
  // texture's whole range included.
  return vec4f(
    luma - usm_blur,            // high-pass: everything finer than the radius
    texture_lo - texture_hi,    // band-pass: the fine scales
    texture_hi - clarity_blur,  // band-pass: the broad scales above them
    usm_blur,
  );
}
