// Written by calcParams.wgsl, read by the two shaders of the sharpness stage.
// Kept apart from Params so neither struct carries fields its readers never
// touch. The binding stays with the shader.
struct SharpParams {
    usm_amount:           f32,   //  0  (1.0 = add the full band)
    usm_sigma:            f32,   //  4  (texture pixels, already floored)
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
