// src-native/src/ui/icons.rs
//
// The bitmap glyphs the interface carries: the mode toggle's sun and bulb,
// and the app icon, kept as a white silhouette so it can be inked like text.

use egui::{ColorImage, TextureHandle, TextureOptions};

pub struct Icons {
    pub sun: TextureHandle,
    pub bulb: TextureHandle,
    pub app: TextureHandle,
}

fn load(ctx: &egui::Context, name: &str, bytes: &[u8], silhouette: bool) -> TextureHandle {
    let img = image::load_from_memory(bytes).expect("bundled icon").to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels: Vec<egui::Color32> = img
        .pixels()
        .map(|p| {
            if silhouette {
                egui::Color32::from_white_alpha(p[3])
            } else {
                egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
            }
        })
        .collect();
    ctx.load_texture(name, ColorImage { size, source_size: egui::vec2(size[0] as f32, size[1] as f32), pixels }, TextureOptions::LINEAR)
}

impl Icons {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            sun: load(ctx, "sun", include_bytes!("../../assets/sun_icon.png"), false),
            bulb: load(ctx, "bulb", include_bytes!("../../assets/lightbulb_icon.png"), false),
            app: load(ctx, "app", include_bytes!("../../assets/favicon.png"), true),
        }
    }
}
