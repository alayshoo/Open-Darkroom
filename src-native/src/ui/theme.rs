// src-native/src/ui/theme.rs
//
// palette.css, and the fonts. The darkroom theme is a second palette the
// whole interface is blended towards, as the CSS transitions every colour
// over half a second.

use egui::{Color32, FontFamily, FontId};

pub fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn hexa(v: u32) -> Color32 {
    Color32::from_rgba_unmultiplied((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

pub fn black(a: f32) -> Color32 {
    rgba(0, 0, 0, a)
}

pub fn white(a: f32) -> Color32 {
    rgba(255, 255, 255, a)
}

/// Mix in sRGB, as CSS interpolates colour transitions.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let a = a.to_srgba_unmultiplied();
    let b = b.to_srgba_unmultiplied();
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3]))
}

/// A colour at a fraction of its own opacity.
pub fn fade(c: Color32, opacity: f32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * opacity.clamp(0.0, 1.0)).round() as u8)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg1: Color32,
    pub bg2: Color32,
    pub bg3: Color32,
    pub bg4: Color32,
    pub bg5: Color32,
    pub color1: Color32,
    pub color1s: Color32,
    pub color2: Color32,
    pub color3: Color32,
    pub color4: Color32,
    pub color5: Color32,
    pub slider_track: Color32,
    pub chrome_ink: Color32,
    pub scrim_strength: f32,
    pub canvas_backdrop: Color32,
    pub glass_tint: Color32,
}

pub const NORMAL: Palette = Palette {
    bg1: Color32::from_rgb(0x00, 0x00, 0x00),
    bg2: Color32::from_rgb(0x10, 0x10, 0x10),
    bg3: Color32::from_rgb(0x1a, 0x1a, 0x1a),
    bg4: Color32::from_rgb(0x1f, 0x1f, 0x1f),
    bg5: Color32::from_rgb(0x26, 0x26, 0x26),
    color1: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    color1s: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    color2: Color32::from_rgb(0xb3, 0xb3, 0xb3),
    color3: Color32::from_rgb(0x63, 0x63, 0x63),
    color4: Color32::from_rgb(0x4c, 0x4c, 0x4c),
    color5: Color32::from_rgb(0x30, 0x30, 0x30),
    slider_track: Color32::from_rgb(0x63, 0x63, 0x63),
    chrome_ink: Color32::from_rgb(0xff, 0xff, 0xff),
    scrim_strength: 0.6,
    canvas_backdrop: Color32::from_rgb(0x26, 0x26, 0x26),
    glass_tint: Color32::from_rgba_premultiplied(0, 0, 0, 148),
};

pub const DARKROOM: Palette = Palette {
    bg1: Color32::from_rgb(0x00, 0x00, 0x00),
    bg2: Color32::from_rgb(0x00, 0x00, 0x00),
    bg3: Color32::from_rgb(0x0d, 0x0d, 0x0d),
    bg4: Color32::from_rgb(0x14, 0x14, 0x14),
    bg5: Color32::from_rgb(0x1e, 0x1e, 0x1e),
    color1: Color32::from_rgb(0xff, 0x0a, 0x0a),
    color1s: Color32::from_rgb(0xff, 0x99, 0x99),
    color2: Color32::from_rgb(0xff, 0x0a, 0x0a),
    color3: Color32::from_rgb(0xff, 0x99, 0x99),
    // Not overridden by the darkroom theme.
    color4: Color32::from_rgb(0x4c, 0x4c, 0x4c),
    color5: Color32::from_rgb(0x30, 0x30, 0x30),
    slider_track: Color32::from_rgb(0x43, 0x00, 0x00),
    chrome_ink: Color32::from_rgb(0xff, 0x0a, 0x0a),
    scrim_strength: 0.88,
    canvas_backdrop: Color32::from_rgb(0x1f, 0x1f, 0x1f),
    glass_tint: Color32::from_rgba_premultiplied(0, 0, 0, 199),
};

impl Palette {
    pub fn blend(t: f32) -> Palette {
        let (a, b) = (NORMAL, DARKROOM);
        Palette {
            bg1: mix(a.bg1, b.bg1, t),
            bg2: mix(a.bg2, b.bg2, t),
            bg3: mix(a.bg3, b.bg3, t),
            bg4: mix(a.bg4, b.bg4, t),
            bg5: mix(a.bg5, b.bg5, t),
            color1: mix(a.color1, b.color1, t),
            color1s: mix(a.color1s, b.color1s, t),
            color2: mix(a.color2, b.color2, t),
            color3: mix(a.color3, b.color3, t),
            color4: a.color4,
            color5: a.color5,
            slider_track: mix(a.slider_track, b.slider_track, t),
            chrome_ink: mix(a.chrome_ink, b.chrome_ink, t),
            scrim_strength: a.scrim_strength + (b.scrim_strength - a.scrim_strength) * t,
            canvas_backdrop: a.canvas_backdrop,
            glass_tint: Color32::from_rgba_premultiplied(0, 0, 0, (148.0 + (199.0 - 148.0) * t).round() as u8),
        }
    }
}

/// Figtree at the weights the CSS uses, and Outfit for the settings heading.
#[derive(Clone, Copy)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

pub fn font(size: f32, weight: Weight) -> FontId {
    let name = match weight {
        Weight::Regular => "fig400",
        Weight::Medium => "fig500",
        Weight::SemiBold => "fig600",
        Weight::Bold => "fig700",
    };
    FontId::new(size, FontFamily::Name(name.into()))
}

pub fn outfit(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("outfit700".into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn install_fonts(ctx: &egui::Context) {
    use std::sync::Arc;
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("Figtree-400", include_bytes!("../../assets/fonts/Figtree-400.ttf"));
    add("Figtree-500", include_bytes!("../../assets/fonts/Figtree-500.ttf"));
    add("Figtree-600", include_bytes!("../../assets/fonts/Figtree-600.ttf"));
    add("Figtree-700", include_bytes!("../../assets/fonts/Figtree-700.ttf"));
    add("Figtree-ext-400", include_bytes!("../../assets/fonts/Figtree-ext-400.ttf"));
    add("Figtree-ext-500", include_bytes!("../../assets/fonts/Figtree-ext-500.ttf"));
    add("Figtree-ext-600", include_bytes!("../../assets/fonts/Figtree-ext-600.ttf"));
    add("Figtree-ext-700", include_bytes!("../../assets/fonts/Figtree-ext-700.ttf"));
    add("Outfit-700", include_bytes!("../../assets/fonts/Outfit-700.ttf"));

    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    for w in [400, 500, 600, 700] {
        let mut list = vec![format!("Figtree-{w}"), format!("Figtree-ext-{w}")];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(format!("fig{w}").into()), list);
    }
    let mut outfit = vec!["Outfit-700".to_string(), "Figtree-700".to_string()];
    outfit.extend(fallback.iter().cloned());
    fonts.families.insert(FontFamily::Name("outfit700".into()), outfit);
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "Figtree-400".into());
    ctx.set_fonts(fonts);
}
