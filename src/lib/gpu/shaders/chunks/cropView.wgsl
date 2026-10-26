// The crop, as a map from output pixels back into the image: the source texel
// coordinate of output point p is origin + p.x * axis_x + p.y * axis_y.
// Mirrors cropView() in src/lib/types/crop.ts and src-tauri/src/crop.rs.
struct CropView {
  origin: vec2f,
  axis_x: vec2f,
  axis_y: vec2f,
  size:   vec2f,   // output extent in pixels
  // Output pixels outside this box are shaded. Only the crop tool's preview
  // draws a box; everything else passes one that covers the whole output.
  keep_min: vec2f,
  keep_max: vec2f,
  // Output pixels this maps outside the unit square are left clear, which is
  // how the preview draws nothing past the edges of the image.
  clip_origin: vec2f,
  clip_axis_x: vec2f,
  clip_axis_y: vec2f,
}
