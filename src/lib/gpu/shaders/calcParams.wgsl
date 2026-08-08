// src/lib/gpu/shaders/calcParams.wgsl
//
// Compute shader that calculates processing matrices from raw slider values.
// Runs once per slider change (single invocation) to produce the Params struct
// consumed by develop.wgsl and histogram.wgsl.

struct Sliders {
    invert:            f32,   //  0   (0.0 or 1.0)
    red_black_point:   f32,   //  4   (0–255)
    green_black_point: f32,   //  8
    blue_black_point:  f32,   // 12
    red_white_point:   f32,   // 16
    green_white_point: f32,   // 20
    blue_white_point:  f32,   // 24
    rgb_output_black:  f32,   // 28
    rgb_output_white:  f32,   // 32
    red_gamma:         f32,   // 36
    green_gamma:       f32,   // 40
    blue_gamma:        f32,   // 44
    wb_temp:           f32,   // 48
    wb_tint:           f32,   // 52
    exposure:          f32,   // 56
    contrast:          f32,   // 60
    brightness:        f32,   // 64
    highlights:        f32,   // 68
    shadows:           f32,   // 72
    whites:            f32,   // 76
    blacks:            f32,   // 80
    saturation:        f32,   // 84
    vibrance:          f32,   // 88
    hue:               f32,   // 92
}                             // total: 96 bytes

struct Params {
    dr_matrix:      mat4x4<f32>,  //   0 — 64 bytes
    red_gamma:      f32,          //  64
    green_gamma:    f32,          //  68
    blue_gamma:     f32,          //  72
    out_black:      f32,          //  76
    out_scale:      f32,          //  80
    _pad1:          f32,          //  84
    _pad2:          f32,          //  88
    _pad3:          f32,          //  92  (align wb_matrix to 96)
    wb_matrix:      mat3x3<f32>,  //  96 — 48 bytes
    exposure:       f32,          // 144
    contrast:       f32,          // 148
    brightness:     f32,          // 152
    highlights:     f32,          // 156
    shadows:        f32,          // 160
    whites:         f32,          // 164
    blacks:         f32,          // 168
    _pad4:          f32,          // 172  (align hueSat_matrix to 176)
    hueSat_matrix:  mat4x4<f32>,  // 176 — 64 bytes
    vibrance:       f32,          // 240
    _pad5:          f32,          // 244
    _pad6:          f32,          // 248
    _pad7:          f32,          // 252
}                                 // total: 256 bytes

@group(0) @binding(0) var<uniform> sliders: Sliders;
@group(0) @binding(1) var<storage, read_write> params: Params;


// ── Constants ────────────────────────────────────────────────────────────────

const PI: f32 = 3.14159265358979323846;

// Bradford cone-space transform (column-major, matching WGSL mat3x3 layout)
const M_BRADFORD = mat3x3<f32>(
    vec3f( 0.8951, -0.7502,  0.0389),
    vec3f( 0.2664,  1.7135, -0.0685),
    vec3f(-0.1614,  0.0367,  1.0296),
);

const M_BRADFORD_INV = mat3x3<f32>(
    vec3f( 0.9869929,  0.4323053, -0.0085287),
    vec3f(-0.1470543,  0.5183603,  0.0400428),
    vec3f( 0.1599627,  0.0492912,  0.9684867),
);

// Linear sRGB <-> XYZ D65 (column-major)
const M_RGB_TO_XYZ = mat3x3<f32>(
    vec3f(0.4124564, 0.2126729, 0.0193339),
    vec3f(0.3575761, 0.7151522, 0.1191920),
    vec3f(0.1804375, 0.0721750, 0.9503041),
);

// Must be the inverse of M_RGB_TO_XYZ to the same precision: at the neutral
// reference temperature the adaptation collapses to M_XYZ_TO_RGB * M_RGB_TO_XYZ,
// so any truncation here shows up as a tint on an image nobody white-balanced.
const M_XYZ_TO_RGB = mat3x3<f32>(
    vec3f( 3.2404542, -0.9692660,  0.0556434),
    vec3f(-1.5371385,  1.8760108, -0.2040259),
    vec3f(-0.4985314,  0.0415560,  1.0572252),
);

const MIN_GAMMA: f32 = 0.01;

// Smallest black-to-white separation the darkroom matrix will divide by.
// Coincident points would otherwise give an infinite slope, and the offset term
// (-black * slope) an outright NaN, poisoning every pixel of the image.
const MIN_POINT_RANGE: f32 = 1e-6;

// BT.709 luminance coefficients
const LR: f32 = 0.2126;
const LG: f32 = 0.7152;
const LB: f32 = 0.0722;

// ── Tone-region amplitude budget ─────────────────────────────────────────────
//
// Highlights, shadows, whites and blacks are additive offsets gated by a
// smoothstep mask, so the developed tone is `x + A·mask(x)` with slope
// `1 - A·mask'(x)`. `smoothstep(0, W, x)` peaks at slope `1.5/W`, so a single
// control folds the tone curve back on itself once `A > W/1.5` — 0.167 for the
// quarter-width masks and 0.333 for the half-width ones. The masks also overlap:
// blacks and shadows are both active below linear 0.25, and their slopes add, so
// the budget has to be shared rather than spent twice.
//
// The binding case is the peak of the black mask at x = 0.125, where the shadow
// mask contributes slope 2.25 and the black mask 6.0:
//
//     2.25 · MAX_WIDE + 6.0 · MAX_NARROW  =  0.45 + 0.42  =  0.87  <  1
//
// so every combination of the four stays monotone with about 13% to spare. The
// sliders are -100..100 in the UI, like every other slider in the app, and the
// rails map to these amplitudes. Nothing is given up by being provably safe:
// at the rail, shadows still takes sRGB 40 to 80 and highlights takes 210 to
// 249. Blacks and whites move less because their masks are narrower, which is
// the same division of labour Lightroom has — the endpoints themselves belong to
// the output black and white point sliders.
const MAX_WIDE_REGION:   f32 = 0.20;   // highlights, shadows (mask width 0.5)
const MAX_NARROW_REGION: f32 = 0.07;   // whites, blacks      (mask width 0.25)

// Brightness is a uniform shift in encoded code values, so it is monotone at any
// amplitude; this is a judgement about feel. A quarter of the range is already a
// drastic move, and the slider needed rescaling regardless: as a linear-light
// offset on a -1..1 range, +0.2 took sRGB 32 to 128.
const MAX_BRIGHTNESS: f32 = 0.25;


// ── CCT -> XYZ (Planckian locus, Kim et al.) ────────────────────────────────

fn cct_to_xyz(temp_raw: f32) -> vec3f {
    let T = clamp(temp_raw, 1000.0, 25000.0);

    var x: f32;
    if (T <= 4000.0) {
        x = -0.2661239e9 / (T * T * T)
          - 0.234358e6  / (T * T)
          + 0.8776956e3 / T
          + 0.17991;
    } else {
        x = -3.0258469e9 / (T * T * T)
          + 2.1070379e6  / (T * T)
          + 0.2226347e3  / T
          + 0.24039;
    }

    var y: f32;
    if (T <= 4000.0) {
        y = -1.1063814 * x * x * x
          - 1.3481102 * x * x
          + 2.1855583 * x
          - 0.2021968;
    } else {
        y =  3.081758  * x * x * x
          - 5.8733867 * x * x
          + 3.75113   * x
          - 0.3700148;
    }

    return vec3f(x / y, 1.0, (1.0 - x - y) / y);
}


// ── Darkroom matrix (black/white point remap + optional inversion) ──────────

// Sliders are expressed in sRGB (0-255). The GPU texture is already linearised,
// so convert to linear before building the remap matrix.
fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        return s / 12.92;
    } else {
        return pow((s + 0.055) / 1.055, 2.4);
    }
}

// Keep a black-to-white span away from zero without flipping its direction.
// The resulting slope is steep enough to act as a hard threshold, which is the
// sensible reading of "black and white points are the same value".
fn safe_range(range: f32) -> f32 {
    if abs(range) >= MIN_POINT_RANGE {
        return range;
    }
    // sign(0.0) is 0.0, so bias exact coincidence to the positive side.
    return select(MIN_POINT_RANGE, -MIN_POINT_RANGE, range < 0.0);
}

// Maps each channel's black point to 0 and its white point to 1, flipping the
// direction when the negative is being inverted. The output black/white pair is
// deliberately *not* folded in here — it is applied after gamma, in step 3 of
// the develop chain, so that the two output sliders remain the true endpoints of
// the result. See the comment on `out_black` in main().
fn calc_darkroom_matrix() -> mat4x4<f32> {
    let rbp = srgb_to_linear(sliders.red_black_point   / 255.0);
    let gbp = srgb_to_linear(sliders.green_black_point / 255.0);
    let bbp = srgb_to_linear(sliders.blue_black_point  / 255.0);
    let rwp = srgb_to_linear(sliders.red_white_point   / 255.0);
    let gwp = srgb_to_linear(sliders.green_white_point / 255.0);
    let bwp = srgb_to_linear(sliders.blue_white_point  / 255.0);

    let invert = sliders.invert > 0.5;
    let start  = select(0.0, 1.0, invert);
    let dir    = select(1.0, -1.0, invert);

    let sR = dir / safe_range(rwp - rbp);
    let sG = dir / safe_range(gwp - gbp);
    let sB = dir / safe_range(bwp - bbp);

    let oR = -rbp * sR + start;
    let oG = -gbp * sG + start;
    let oB = -bbp * sB + start;

    return mat4x4<f32>(
        vec4f(sR,  0.0, 0.0, 0.0),
        vec4f(0.0, sG,  0.0, 0.0),
        vec4f(0.0, 0.0, sB,  0.0),
        vec4f(oR,  oG,  oB,  1.0),
    );
}


// ── White balance matrix (Bradford CAT + tint) ──────────────────────────────

fn calc_wb_matrix() -> mat3x3<f32> {
    let src_xyz = cct_to_xyz(sliders.wb_temp);
    let dst_xyz = cct_to_xyz(5500.0);   // neutral reference

    let src_cone = M_BRADFORD * src_xyz;
    let dst_cone = M_BRADFORD * dst_xyz;

    // Diagonal scale: adapt src -> dst
    let scale = dst_cone / src_cone;
    let diag_scale = mat3x3<f32>(
        vec3f(scale.x, 0.0,     0.0),
        vec3f(0.0,     scale.y, 0.0),
        vec3f(0.0,     0.0,     scale.z),
    );

    // Full CAT in XYZ space
    let M_CAT_XYZ = M_BRADFORD_INV * diag_scale * M_BRADFORD;

    // Bring into linear sRGB space
    let M_CAT_RGB = M_XYZ_TO_RGB * M_CAT_XYZ * M_RGB_TO_XYZ;

    // Tint: perpendicular to Planckian locus (magenta/green axis)
    let tint_scale = pow(2.0, -sliders.wb_tint / 100.0);
    let M_TINT = mat3x3<f32>(
        vec3f(1.0,        0.0, 0.0),
        vec3f(0.0, tint_scale, 0.0),
        vec3f(0.0,        0.0, 1.0),
    );

    let M_WB = M_TINT * M_CAT_RGB;

    // Normalise so the neutral axis keeps its luminance.
    //
    // Neither factor above preserves brightness on its own. The tint term is the
    // worse of the two: it scales green, which carries 72% of luminance, so tint
    // +100 costs two thirds of a stop and tint -100 gains three quarters of one.
    // Temperature drifts less but still drifts. Uncompensated, that makes white
    // balance a hidden exposure control — you correct a cast, the image changes
    // brightness, you correct the exposure, and the histogram never settles.
    //
    // A single scalar is the right correction: it cancels the luminance change
    // exactly while leaving every channel ratio — the actual colour decision —
    // untouched. Because luma(1,1,1) = LR + LG + LB = 1, dividing by the luma of
    // the transformed white is what makes a neutral come out at its input
    // luminance.
    let neutral = M_WB * vec3f(1.0, 1.0, 1.0);
    let neutral_luma = dot(vec3f(LR, LG, LB), neutral);

    return M_WB * (1.0 / max(neutral_luma, 1e-6));
}


// ── Hue + Saturation matrix ─────────────────────────────────────────────────

fn calc_hue_sat_matrix() -> mat4x4<f32> {
    let saturation = 1.0 + sliders.saturation / 100.0;
    let hue        = sliders.hue * PI / 180.0;

    let sr = (1.0 - saturation) * LR;
    let sg = (1.0 - saturation) * LG;
    let sb = (1.0 - saturation) * LB;

    // Saturation matrix (column-major)
    let sat_mat = mat4x4<f32>(
        vec4f(sr + saturation, sr,              sr,              0.0),
        vec4f(sg,              sg + saturation, sg,              0.0),
        vec4f(sb,              sb,              sb + saturation, 0.0),
        vec4f(0.0,             0.0,             0.0,             1.0),
    );

    if (hue == 0.0) {
        return sat_mat;
    }

    // Hue rotation around (1,1,1) axis (Rodrigues' formula)
    let c  = cos(hue);
    let s  = sin(hue);
    let k  = (1.0 - c) / 3.0;
    let sq = sqrt(1.0 / 3.0);

    let hue_mat = mat4x4<f32>(
        vec4f(c + k,      k - sq * s, k + sq * s, 0.0),
        vec4f(k + sq * s, c + k,      k - sq * s, 0.0),
        vec4f(k - sq * s, k + sq * s, c + k,      0.0),
        vec4f(0.0,        0.0,        0.0,        1.0),
    );

    return hue_mat * sat_mat;
}


// ── Main entry point (single invocation) ─────────────────────────────────────

@compute @workgroup_size(1)
fn main() {
    params.dr_matrix     = calc_darkroom_matrix();
    params.red_gamma     = max(sliders.red_gamma,   MIN_GAMMA);
    params.green_gamma   = max(sliders.green_gamma, MIN_GAMMA);
    params.blue_gamma    = max(sliders.blue_gamma,  MIN_GAMMA);

    // Output black/white, as an affine map applied after gamma rather than
    // inside dr_matrix. The order that matters is input remap → gamma → output
    // remap, which is what a Levels dialog and a scanner both do. Folded into
    // dr_matrix the output pair was applied *before* the power, and gamma then
    // dragged both endpoints somewhere else entirely: output black 80 with gamma
    // 2.2 put pure black at sRGB 153, and output white 200 put pure white at 228
    // — the two sliders whose entire job is to pin the endpoints no longer did.
    //
    // Collapsing the pair (ow == ob) still yields a flat field at ob, and the
    // default 0/255 gives out_scale exactly 1 and out_black exactly 0, so the
    // neutral chain stays a bit-exact no-op.
    let ob = srgb_to_linear(sliders.rgb_output_black / 255.0);
    let ow = srgb_to_linear(sliders.rgb_output_white / 255.0);
    params.out_black     = ob;
    params.out_scale     = ow - ob;

    params._pad1         = 0.0;
    params._pad2         = 0.0;
    params._pad3         = 0.0;
    params.wb_matrix     = calc_wb_matrix();
    params.exposure      = sliders.exposure;
    params.contrast      = sliders.contrast / 100.0;
    params.brightness    = sliders.brightness / 100.0 * MAX_BRIGHTNESS;
    params.highlights    = sliders.highlights / 100.0 * MAX_WIDE_REGION;
    params.shadows       = sliders.shadows    / 100.0 * MAX_WIDE_REGION;
    params.whites        = sliders.whites     / 100.0 * MAX_NARROW_REGION;
    params.blacks        = sliders.blacks     / 100.0 * MAX_NARROW_REGION;
    params._pad4         = 0.0;
    params.hueSat_matrix = calc_hue_sat_matrix();
    params.vibrance      = sliders.vibrance / 100.0;
    params._pad5         = 0.0;
    params._pad6         = 0.0;
    params._pad7         = 0.0;
}
