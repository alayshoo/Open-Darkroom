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
@group(0) @binding(2) var<uniform> params: Params;

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

  // 4. Exposure
  rgb *= pow(2.0, params.exposure);

  // 5. Contrast — sigmoid around mid-gray
  let contrasted = 1.0 / (1.0 + exp(-(rgb - 0.5) * 10.0));
  rgb = mix(rgb, contrasted, params.contrast);

  // 6. Brightness — additive offset
  rgb += params.brightness;

  // 7. Highlights / Shadows / Whites / Blacks
  let luma_tone = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));

  let highlightMask = smoothstep(0.5, 1.0, luma_tone);
  let shadowMask    = 1.0 - smoothstep(0.0, 0.5, luma_tone);
  let whiteMask     = smoothstep(0.75, 1.0, luma_tone);
  let blackMask     = 1.0 - smoothstep(0.0, 0.25, luma_tone);

  rgb += params.highlights * highlightMask;
  rgb += params.shadows    * shadowMask;
  rgb += params.whites     * whiteMask;
  rgb += params.blacks     * blackMask;

  // 8. Hue + Saturation (pre-composed matrix from CPU)
  rgb = (params.hueSat_matrix * vec4f(rgb, 1.0)).xyz;

  // 9. Vibrance — boost under-saturated pixels more
  let luma_vib = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));
  let maxC = max(rgb.r, max(rgb.g, rgb.b));
  let minC = min(rgb.r, min(rgb.g, rgb.b));
  let pixelSat = (maxC - minC) / (maxC + 0.001);
  let vibranceAmount = params.vibrance * (1.0 - pixelSat);
  rgb = mix(vec3f(luma_vib), rgb, 1.0 + vibranceAmount);

  // Clamp + linear → sRGB
  let clamped = clamp(rgb, vec3f(0.0), vec3f(1.0));
  let cutoff = vec3f(0.0031308);
  let lo = clamped * 12.92;
  let hi = 1.055 * pow(clamped, vec3f(1.0 / 2.4)) - 0.055;
  let srgb = select(hi, lo, clamped < cutoff);

  return vec4f(srgb, color.a);
}