// src/lib/gpu/shaders/composite.wgsl
//
// Applies the sharpness detail bands to the developed image. Sits between
// develop.wgsl and colorSpaceEncode.wgsl and operates on the perceptual working
// signal, which is where sharpening belongs: a fixed halo amplitude in linear
// light would mean something different at every brightness, and applying it
// after the output transform would tie the edit to the destination colour space.

// Must match SharpParams in calcParams.wgsl and calcSharpTexture.wgsl
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

@group(0) @binding(0) var developedTexture: texture_2d<f32>;
@group(0) @binding(1) var detailTexture: texture_2d<f32>;
@group(0) @binding(2) var<storage, read> sharp: SharpParams;

// BT.709 luminance coefficients
const LUMA = vec3f(0.2126, 0.7152, 0.0722);

// Smallest luma the band is allowed to be divided by. Below this the colour has
// almost no direction left to scale along and the gain runs away; the shadows
// are also where sharpening does the least good and the most harm.
const MIN_LUMA: f32 = 0.01;

// Ceiling on how far clarity may move a single pixel, in encoded code values.
//
// Clarity's radius is tens of pixels, so its halo is tens of pixels wide, and an
// unbounded one is exactly the over-processed look the effect is infamous for.
// The limit is approached asymptotically rather than clipped, so the slider
// keeps a usable response all the way to the rail instead of flattening at the
// point the limit is hit.
const CLARITY_LIMIT: f32 = 0.15;

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

// A threshold ramps from zero up to its setting rather than sitting at a fixed
// width around it. That is what makes a setting of 0 an exact no-op — the
// smoothstep collapses to a step at the origin, so anything with signal passes
// ungated — and it leaves no hard edge where the gate opens, which would
// otherwise contour along an isoline of the image.
fn gate(value: f32, threshold: f32) -> f32 {
  return smoothstep(0.0, max(threshold, 1e-6), value);
}

// Clarity backs off at the two ends of the range. Its band is a large-radius
// local contrast move, and driving it into the deepest shadows or the brightest
// highlights only crushes them — those endpoints belong to the output black and
// white points. Between the quarter and three-quarter tones it acts in full.
fn midtone_mask(l: f32) -> f32 {
  return smoothstep(0.0, 0.25, l) * (1.0 - smoothstep(0.75, 1.0, l));
}

// Approaches ±limit asymptotically instead of clipping at it.
fn soft_clip(x: f32, limit: f32) -> f32 {
  return limit * tanh(x / limit);
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  // Both textures are the same size as this target, so fragment coordinates
  // index them directly. Loading rather than sampling keeps the mapping exact —
  // no filtering is wanted and no half-texel offset can creep in between stages.
  let coord = vec2i(position.xy);
  let developed = textureLoad(developedTexture, coord, 0);
  let detail = textureLoad(detailTexture, coord, 0);

  let usm_band = detail.r;
  let texture_band = detail.g;
  let clarity_band = detail.b;
  let blurred_luma = detail.a;

  let luma = dot(developed.rgb, LUMA);

  // Unsharp mask.
  //
  // The luma gate keeps sharpening out of the shadows, where the signal it would
  // amplify is mostly noise. It reads the *blurred* luma rather than the pixel's
  // own: the unblurred value carries the very noise being guarded against, so a
  // gate driven by it would flicker open and shut across a dark region and
  // sharpen exactly the speckles it was meant to protect.
  //
  // The detail gate does the same job in the other axis, suppressing the
  // smallest bands wherever they occur. Only the unsharp mask carries these —
  // texture and clarity are band-limited by construction, which is what they
  // have instead of a threshold.
  let usm_delta = sharp.usm_amount * usm_band
                * gate(blurred_luma, sharp.usm_luma_threshold)
                * gate(abs(usm_band), sharp.usm_detail_threshold);

  // Texture — the band is already restricted to a range of scales, so it needs
  // no shaping of its own.
  let texture_delta = sharp.texture_amount * texture_band;

  // Clarity — the band above texture's, masked away from the endpoints and
  // bounded, for the reasons on midtone_mask and CLARITY_LIMIT.
  let clarity_delta = soft_clip(
    sharp.clarity_amount * clarity_band * midtone_mask(luma),
    CLARITY_LIMIT,
  );

  // Scale the colour by the change in its luma, rather than adding the bands to
  // each channel. Both move the luma by exactly the same amount — the BT.709
  // weights sum to one — but a multiply preserves every channel ratio, so hue
  // and saturation survive untouched. Adding instead moves the colour along the
  // neutral axis, which washes out the bright side of every edge and
  // over-saturates the dark side: a chroma ripple that grows with the amount.
  let delta = usm_delta + texture_delta + clarity_delta;
  let rgb = developed.rgb * ((luma + delta) / max(luma, MIN_LUMA));

  return vec4f(rgb, developed.a);
}
