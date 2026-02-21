// src/lib/gpu/shaders/histogram_compute.wgsl



@group(0) @binding(0) var inputTexture: texture_2d<f32>;        // image input

// these mush match the ones in develop.wgsl
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
@group(0) @binding(1) var<uniform> params: Params;  // same struct as develop.wgsl

// Variables to calculate data into.
@group(0) @binding(2) var<storage, read_write> bins_r: array<atomic<u32>, 256>;
@group(0) @binding(3) var<storage, read_write> bins_g: array<atomic<u32>, 256>;
@group(0) @binding(4) var<storage, read_write> bins_b: array<atomic<u32>, 256>;



@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  let dimensions = textureDimensions(inputTexture);
  if (gid.x >= dimensions.x || gid.y >= dimensions.y) { return; }

  var rgb = textureLoad(inputTexture, vec2u(gid.x, gid.y), 0).rgb;

  // ===================== Apply same adjustment chain as develop.wgsl =====================
    // sRGB → linear
    let lo_in = rgb / 12.92;
    let hi_in = pow((rgb + vec3f(0.055)) / 1.055, vec3f(2.4));
    rgb = select(hi_in, lo_in, rgb <= vec3f(0.04045));

    // 1. White Balance
    let temp = params.wb_temp;
    let tint = params.wb_tint;
    rgb.r *= 1.0 + temp * 0.4;
    rgb.b *= 1.0 - temp * 0.4;
    rgb.g *= 1.0 - tint * 0.4;
    let wbLumaCorrection = 1.0 / (1.0 + abs(temp) * 0.13 + abs(tint) * 0.1);
    rgb *= wbLumaCorrection;

    // 2. Exposure
    rgb *= pow(2.0, params.exposure);

    // 3. Contrast
    rgb = 1.0 / (1.0 + exp(-params.contrast * (rgb - 0.5) * 10.0)) ;

    // 4. Brightness
    rgb += params.brightness;

    // Luminance used by both vibrance and saturation (Rec.709 weights)
    let luma = dot(rgb, vec3f(0.2126, 0.7152, 0.0722));

    // 5. Vibrance
    let maxC = max(rgb.r, max(rgb.g, rgb.b));
    let minC = min(rgb.r, min(rgb.g, rgb.b));
    let pixelSat = (maxC - minC) / (maxC + 0.001);  // 0 = gray, 1 = vivid
    let vibranceAmount = params.vibrance * (1.0 - pixelSat);  // stronger on muted pixels
    rgb = mix(vec3f(luma), rgb, 1.0 + vibranceAmount);

    // 6. Saturation
    rgb = mix(vec3f(luma), rgb, 1.0 + params.saturation);
  // ============================= (update if it ever changes) =============================

  let r = u32(clamp(rgb.r, 0.0, 1.0) * 255.0);
  let g = u32(clamp(rgb.g, 0.0, 1.0) * 255.0);
  let b = u32(clamp(rgb.b, 0.0, 1.0) * 255.0);

  atomicAdd(&bins_r[r], 1u);
  atomicAdd(&bins_g[g], 1u);
  atomicAdd(&bins_b[b], 1u);
}