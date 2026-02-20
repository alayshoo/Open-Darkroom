// src/lib/gpu/shaders/develop.wgsl

// A "binding" is how the shader accesses resources from JavaScript.
// group(0) = the bind group index, binding(N) = slot within that group.

// The image texture we're processing
@group(0) @binding(0) var inputTexture: texture_2d<f32>;

// A sampler defines how to read the texture (filtering, wrapping, etc.)
@group(0) @binding(1) var texSampler: sampler;

// Uniform buffer = small, read-only data shared across all pixels.
// This is where our values live.
// Params must equate to a multiple of 16 bytes, each f32 is 4 bytes
struct Params {
  wb_temp: f32,
  wb_tint: f32,
  exposure: f32,
  contrast: f32,
  brightness: f32,
  saturation: f32,
  vibrance: f32,
  _pad: f32,  // pad to 32 bytes (multiple of 16)
}
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

  // 1. White Balance
  //    Temperature: shift along blue ↔ yellow axis
  //    Tint: shift along green ↔ magenta axis
  //    We scale the channels directly. The coefficients are tuned
  //    so ±1 gives a strong but not extreme shift.
  let temp = params.wb_temp;
  let tint = params.wb_tint;
  rgb.r *= 1.0 + temp * 0.4;
  rgb.b *= 1.0 - temp * 0.4;
  rgb.g *= 1.0 - tint * 0.4;
  // Compensate luminance shift from WB (keep overall brightness stable)
  let wbLumaCorrection = 1.0 / (1.0 + abs(temp) * 0.13 + abs(tint) * 0.1);
  rgb *= wbLumaCorrection;

  // 2. Exposure: multiply by 2^exposure
  rgb *= pow(2.0, params.exposure);

  // 3. Contrast: pivot around mid-gray
  let contrastFactor = 1.0 + params.contrast;
  rgb = vec3f(0.5) + (rgb - vec3f(0.5)) * contrastFactor;
  // S-curve contrast (future upgrade)
  // rgb = 1.0 / (1.0 + exp(-params.contrast * (rgb - 0.5) * 10.0)) ;

  // 4. Brightness: additive offset
  rgb += params.brightness;

  // Luminance used by both vibrance and saturation (Rec.709 weights)
  let luma = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));

  // 5. Vibrance: "smart" saturation
  //    Boosts under-saturated pixels more, leaves vivid pixels alone.
  //    Measures per-pixel saturation as the spread of RGB channels.
  let maxC = max(rgb.r, max(rgb.g, rgb.b));
  let minC = min(rgb.r, min(rgb.g, rgb.b));
  let pixelSat = (maxC - minC) / (maxC + 0.001);  // 0 = gray, 1 = vivid
  let vibranceAmount = params.vibrance * (1.0 - pixelSat);  // stronger on muted pixels
  rgb = mix(vec3f(luma), rgb, 1.0 + vibranceAmount);

  // 6. Saturation: uniform boost/cut across all colors
  //    -1 = full grayscale, 0 = no change, +1 = 2× saturation
  rgb = mix(vec3f(luma), rgb, 1.0 + params.saturation);

  return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), color.a);
}