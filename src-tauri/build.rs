#[path = "shader_flatten.rs"]
mod shader_flatten;

const SHADERS: &str = "../src/lib/gpu/shaders";

fn main() {
    shader_flatten::flatten_shaders(SHADERS);
    tauri_build::build()
}
