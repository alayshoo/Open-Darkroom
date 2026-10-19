// src/lib/gpu/shaders/develop.wgsl


// group(0) = bind group index, binding(N) = slot within group.

@group(0) @binding(0) var inputTexture: texture_2d<f32>;  // Image being processed

// A sampler defines how to read the texture (filtering, wrapping, etc.)
@group(0) @binding(1) var texSampler: sampler;

// Uniform buffer = small, read-only data shared across all pixels.
//!include "chunks/params.wgsl"
@group(0) @binding(2) var<storage, read> params: Params;

//!include "chunks/luma.wgsl"
//!include "chunks/tone.wgsl"
//!include "chunks/developChain.wgsl"

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
  let rgb = develop(color.rgb);

  // Encode into the perceptual working space and hand off.
  //
  // Deliberately unclamped. Sharpening runs on this signal in composite.wgsl,
  // and colorSpaceEncode.wgsl clamps once at the very end, in the destination
  // gamut. Clamping here instead would flatten the headroom before the stage
  // that can still use it: a negative halo pulls a blown highlight back into
  // range, which it cannot do against an already-flat ceiling.
  return vec4f(encode_srgb(rgb), color.a);
}
