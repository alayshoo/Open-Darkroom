// src/lib/gpu/shaders/colorSpaceEncode.wgsl
//
// Output transform: perceptual working signal → destination colour space.
//
// This is the last stage, and the only one that knows what it is rendering for.
// Four steps: decode the working curve, matrix from the working primaries into
// the destination's, fit the result into the destination gamut, apply the
// destination transfer curve.
//
// Which destination that is comes from the uniform rather than the source. The
// preview renders for the display, an export renders for the space the file
// will be tagged with, and a soft-proofed preview renders for both at once.
//
// The gamut fitting lives here rather than at the end of develop.wgsl so that
// the headroom above white and the out-of-gamut negatives survive the
// sharpening stage — a negative halo can pull a blown highlight back into
// range, which it could not do if the signal had already been flattened against
// the ceiling.
//
// The transform itself lives in chunks/outputTransform.wgsl because
// histogram.wgsl has to apply the same one: a histogram of the working space
// would not describe the picture on screen.

//!include "chunks/tone.wgsl"
//!include "chunks/luma.wgsl"
//!include "chunks/outputTransform.wgsl"
//!include "chunks/cropView.wgsl"

@group(0) @binding(0) var compositeTexture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> outputSpace: OutputSpace;
// The crop is applied here, after sharpening, so the blurs near its edges still
// see the pixels beyond it.
@group(0) @binding(2) var<uniform> crop: CropView;

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

// Bilinear by hand, since the working format cannot be filtered. A point on a
// texel centre returns that texel untouched, which keeps an unrotated crop
// bit-exact.
fn sample_composite(src: vec2f) -> vec4f {
  let p = src - 0.5;
  let base = floor(p);
  let f = p - base;
  let i = vec2i(base);
  let last = vec2i(textureDimensions(compositeTexture)) - 1;

  let c00 = textureLoad(compositeTexture, clamp(i, vec2i(0), last), 0);
  if (all(f == vec2f(0.0))) {
    return c00;
  }
  let c10 = textureLoad(compositeTexture, clamp(i + vec2i(1, 0), vec2i(0), last), 0);
  let c01 = textureLoad(compositeTexture, clamp(i + vec2i(0, 1), vec2i(0), last), 0);
  let c11 = textureLoad(compositeTexture, clamp(i + vec2i(1, 1), vec2i(0), last), 0);
  return mix(mix(c00, c10, f.x), mix(c01, c11, f.x), f.y);
}

// Where a rotated frame reaches past the image: nothing, so the preview's
// backdrop shows through. A crop is kept on the image, so an export never
// lands here.
const OFF_IMAGE = vec4f(0.0);

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let src = crop.origin + position.x * crop.axis_x + position.y * crop.axis_y;
  let extent = vec2f(textureDimensions(compositeTexture));
  if (any(src < vec2f(-0.5)) || any(src > extent + 0.5)) {
    return OFF_IMAGE;
  }
  let c = sample_composite(src);

  // The working curve is undone exactly: encode_srgb and decode_srgb both pass
  // values outside 0..1 through their linear segment, so the headroom and the
  // negatives reach the matrix intact rather than being flattened on the way.
  let working = decode_srgb(c.rgb);

  // Opaque whatever the source's alpha: the preview canvas is premultiplied
  // only so that off-image areas can be left clear.
  return vec4f(to_destination(working, outputSpace), 1.0);
}
