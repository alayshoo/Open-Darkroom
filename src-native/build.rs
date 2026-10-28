#[path = "../src-tauri/shader_flatten.rs"]
mod shader_flatten;

const SHADERS: &str = "../src/lib/gpu/shaders";

fn main() {
    shader_flatten::flatten_shaders(SHADERS);
    println!("cargo:rerun-if-changed=src/gpu/chrome.wgsl");
    println!("cargo:rerun-if-changed=src/gpu/blur.wgsl");
}
