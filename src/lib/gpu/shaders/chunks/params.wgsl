// Written by calcParams.wgsl, read by develop.wgsl and histogram.wgsl. The
// binding that exposes it differs per shader and stays with the shader.
struct Params {
  dr_matrix:      mat4x4<f32>,  //   0 — 64 bytes
  red_gamma:      f32,          //  64
  green_gamma:    f32,          //  68
  blue_gamma:     f32,          //  72
  out_black:      f32,          //  76
  out_scale:      f32,          //  80
  _pad1:          f32,          //  84
  _pad2:          f32,          //  88
  _pad3:          f32,          //  92  (align wb_matrix to 16)
  wb_matrix:      mat3x3<f32>,  //  96 — 48 bytes (3 cols × 16)
  exposure:       f32,          // 144
  contrast:       f32,          // 148
  brightness:     f32,          // 152
  highlights:     f32,          // 156
  shadows:        f32,          // 160
  whites:         f32,          // 164
  blacks:         f32,          // 168
  _pad4:          f32,          // 172  (align hueSat_matrix to 16)
  hueSat_matrix:  mat4x4<f32>,  // 176 — 64 bytes
  vibrance:       f32,          // 240
  rolloff_start:  f32,          // 244  (1 = off)
  rolloff_mode:   f32,          // 248  (0 luminance, 1 per channel)
  _pad7:          f32,          // 252
}                               // total: 256 bytes
