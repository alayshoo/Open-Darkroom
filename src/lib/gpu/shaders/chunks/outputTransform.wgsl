// The output transform: working space → destination, and the gamut it has to
// fit into on the way.
//
// Shared by colorSpaceEncode.wgsl, which renders it, and histogram.wgsl, which
// has to measure the same numbers. A histogram of the working space would
// describe an image nobody is being shown, and would put the clipping warnings
// somewhere other than where the picture clips.
//
// Requires chunks/tone.wgsl for encode_srgb and chunks/luma.wgsl for LUMA.

// Transfer curve ids. Must match `Transfer` in src-tauri/src/output_space.rs
// and TRANSFER in src/lib/gpu/outputSpace.ts.
const TRANSFER_SRGB: u32 = 0u;
const TRANSFER_ADOBE: u32 = 1u;
const TRANSFER_BT2020: u32 = 2u;

const ADOBE_GAMMA: f32 = 563.0 / 256.0;
const BT2020_ALPHA: f32 = 1.0992968;
const BT2020_BETA: f32 = 0.018053969;

// Below this a channel is close enough to the neutral axis that its distance to
// the gamut boundary is not a meaningful limit, and dividing by it would not be
// either.
const GAMUT_EPSILON: f32 = 1e-6;

// Where the render is going, and what it is being proofed against on the way.
//
// Three matrices rather than one: the display is the destination, but a soft
// proof has to detour through the space the file will be written in and come
// back, which is a conversion in each direction.
struct OutputSpace {
  // Linear working space → linear destination, the space being displayed.
  to_destination: mat3x3<f32>,
  // Linear working space → the space being proofed, and back again. Both unused
  // while `proof` is zero.
  to_proof: mat3x3<f32>,
  from_proof: mat3x3<f32>,
  // Which transfer curve the destination encodes with.
  curve: u32,
  // Non-zero while the preview is being proofed against an export space.
  proof: u32,
}

// Pull a colour the destination cannot hold in towards its own luminance.
//
// Hue and luminance are both kept: the colour moves along the straight line to
// the neutral of the same brightness, and stops the moment every channel is in
// range. A colour already inside is returned untouched, so this costs nothing in
// the gamut it is mapping into — unlike a per-channel clamp, which drops the
// most saturated channel first and slides the hue of everything it touches.
//
// All destinations share the working space's white, so the neutral axis of any
// of them is the working-space luminance repeated across the three channels.
fn gamut_compress(rgb: vec3f, luma: f32) -> vec3f {
  // A luminance above white or below black has no neutral inside the gamut to
  // aim at; the nearest one is the endpoint itself.
  let grey = clamp(luma, 0.0, 1.0);
  let offset = rgb - vec3f(grey);

  // How far each channel can travel from the neutral before leaving 0..1, as a
  // fraction of how far it is asking to travel. A channel sitting on the
  // neutral asks for nothing and so limits nothing.
  let headroom = select(vec3f(grey), vec3f(1.0) - vec3f(grey), offset > vec3f(0.0));
  let reach = select(vec3f(1.0), headroom / abs(offset), abs(offset) > vec3f(GAMUT_EPSILON));

  let scale = min(1.0, min(reach.r, min(reach.g, reach.b)));
  return clamp(vec3f(grey) + offset * scale, vec3f(0.0), vec3f(1.0));
}

// Linear destination values → the code values the file or the display expects.
fn encode_transfer(c: vec3f, curve: u32) -> vec3f {
  if (curve == TRANSFER_ADOBE) {
    return pow(max(c, vec3f(0.0)), vec3f(1.0 / ADOBE_GAMMA));
  }
  if (curve == TRANSFER_BT2020) {
    let lo = c * 4.5;
    let hi = BT2020_ALPHA * pow(max(c, vec3f(0.0)), vec3f(0.45)) - (BT2020_ALPHA - 1.0);
    return select(hi, lo, c < vec3f(BT2020_BETA));
  }
  // TRANSFER_SRGB, shared by sRGB and Display P3.
  return encode_srgb(c);
}

// Linear working space → linear destination, fitted to the destination gamut.
fn to_destination_linear(working: vec3f, space: OutputSpace) -> vec3f {
  var signal = working;

  // Soft proof: squeeze the signal through the export space's gamut and bring
  // it straight back, so what reaches the display is what the file will be able
  // to hold rather than what the display could have shown.
  if (space.proof != 0u) {
    signal = space.from_proof * gamut_compress(space.to_proof * signal, dot(signal, LUMA));
  }

  return gamut_compress(space.to_destination * signal, dot(signal, LUMA));
}

// Linear working space → encoded destination, fitted to the destination gamut.
fn to_destination(working: vec3f, space: OutputSpace) -> vec3f {
  return encode_transfer(to_destination_linear(working, space), space.curve);
}
