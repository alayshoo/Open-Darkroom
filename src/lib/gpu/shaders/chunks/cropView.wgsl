// The crop, as a map from output pixels back into the image: the source texel
// coordinate of output point p is origin + p.x * axis_x + p.y * axis_y.
// Mirrors cropView() in src/lib/types/crop.ts and src-tauri/src/crop.rs.
struct CropView {
  origin: vec2f,
  axis_x: vec2f,
  axis_y: vec2f,
  size:   vec2f,   // output extent in pixels
}
