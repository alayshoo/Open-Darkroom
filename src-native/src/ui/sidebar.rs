// src-native/src/ui/sidebar.rs
//
// The side bar: the live histogram, the tools panel with its quick actions and
// the paged strip of sliders, and the footer with export and the mode toggle.

use egui::{pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Ui, UiBuilder};

use super::anim::{EASE, EASE_IN_OUT, LINEAR};
use super::curve_editor;
use super::cx::{shadow_text, Cx, Mask};
use super::path::Placed;
use super::theme::{black, fade, font, hex, white, Weight};
use super::widgets::{self, glass_button, glass_toggle, single_group, Editing, SingleSpec, Stop};
use crate::app::App;
use crate::gpu::chrome::{shadow, GlassStyle, Shadow};
use crate::model::sliders::{hsl_key, n, Sliders, SLIDER_TOLERANCE};

pub const SMALL_SHADOWS: [Shadow; 2] = [
    shadow(6.0, 10.0, 22.0, -2.0, Color32::from_rgba_premultiplied(0, 0, 0, 120)),
    shadow(2.0, 3.0, 8.0, -1.0, Color32::from_rgba_premultiplied(0, 0, 0, 87)),
];

/// A glass panel: its drop shadows, the material, and the lit inner smear.
pub fn panel(cx: &mut Cx, painter: &egui::Painter, rect: Rect, big: bool) {
    let mark = cx.fx.mark();
    if big {
        cx.fx.shadow(rect, 12.0, &shadow(10.0, 16.0, 38.0, -4.0, black(0.51)));
        cx.fx.shadow(rect, 12.0, &shadow(3.0, 5.0, 12.0, -2.0, black(0.36)));
    } else {
        for s in &SMALL_SHADOWS {
            cx.fx.shadow(rect, 12.0, s);
        }
    }
    let mut style = GlassStyle::panel(cx.pal.glass_tint);
    if big {
        style.angle = 146.0;
    }
    cx.fx.glass(rect, [12.0; 4], &style);
    if big {
        cx.fx.inset(rect, 12.0, &shadow(20.0, 30.0, 60.0, -42.0, white(0.17)));
    } else {
        cx.fx.inset(rect, 12.0, &shadow(16.0, 21.0, 50.0, -38.0, white(0.16)));
    }
    cx.fx.flush(painter, mark);
}

/// Takes the pointer over the whole of a panel, as the CSS panel does,
/// so empty glass does not pass clicks through to the image.
pub fn block(ui: &Ui, id: Id, rect: Rect) {
    ui.interact(rect, id, Sense::click_and_drag());
}

const SECTION_TITLE: f32 = 18.0;

/// What a page's sliders committed this frame, applied once it is drawn.
#[derive(Default)]
pub struct Commits {
    pub sliders: Vec<(usize, f64, f64)>,
    pub curves: Option<(crate::model::curves::Curves, crate::model::curves::Curves)>,
    pub reset_mixer: bool,
}

/// A vertical run of controls, with its cursor.
struct Col {
    x: f32,
    y: f32,
    w: f32,
}

fn section_title(cx: &mut Cx, ui: &Ui, col: &mut Col, title: &str) {
    let r = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w, 24.0));
    let painter = cx.painter(ui, r);
    shadow_text(&painter, pos2(col.x, col.y + 12.0), Align2::LEFT_CENTER, title, font(SECTION_TITLE, Weight::SemiBold), cx.pal.color1, 0.35);
    col.y += 24.0 + 6.0;
}

fn separator(cx: &mut Cx, ui: &Ui, col: &mut Col) {
    col.y += 12.0;
    let r = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w - 6.0, 2.0));
    let painter = cx.painter(ui, r);
    let mark = cx.fx.mark();
    cx.fx.fill(r, [1.0; 4], cx.pal.color5);
    cx.fx.flush(&painter, mark);
    col.y += 2.0 + 8.0;
}

#[allow(clippy::too_many_arguments)]
fn slider(cx: &mut Cx, ui: &mut Ui, col: &mut Col, sliders: &mut Sliders, editing: &mut Editing, index: usize, spec: SingleSpec<'_>, commits: &mut Commits) {
    let id = Id::new(("slider", cx.salt, index));
    if let Some((old, new)) = single_group(cx, ui, id, col.x, col.y, col.w, &mut sliders.v[index], &spec, editing) {
        commits.sliders.push((index, old, new));
    }
    col.y += widgets::SINGLE_HEIGHT;
}

/// A section that folds away as svelte's `slide` does: its height runs to
/// nothing and it is clipped on the way.
fn sliding(cx: &mut Cx, ui: &mut Ui, col: &mut Col, id: &str, shown: bool, heights: &mut std::collections::HashMap<&'static str, f32>, key: &'static str, body: impl FnOnce(&mut Cx, &mut Ui, &mut Col)) {
    let t = cx.anim.flag(Id::new(("slide", id)), shown, 0.2, super::anim::Ease(0.215, 0.61, 0.355, 1.0));
    if t <= 0.0 {
        return;
    }
    let full = heights.get(key).copied().unwrap_or(0.0);
    let visible = if t >= 1.0 { f32::INFINITY } else { full * t };
    let clip = Rect::from_min_size(pos2(col.x - 16.0, col.y), vec2(col.w + 40.0, visible.max(0.0).min(1e6))).intersect(ui.clip_rect());
    let mut child = ui.new_child(UiBuilder::new().max_rect(ui.max_rect()));
    child.set_clip_rect(clip);
    let saved = cx.opacity;
    cx.opacity *= (t * 20.0).min(1.0);
    let mut inner = Col { x: col.x, y: col.y, w: col.w };
    let start = inner.y;
    body(cx, &mut child, &mut inner);
    cx.opacity = saved;
    heights.insert(key, inner.y - start);
    col.y += if t >= 1.0 { inner.y - start } else { visible };
}

// ── Pages ─────────────────────────────────────────────────────────────────────

fn page_sharpness(cx: &mut Cx, ui: &mut Ui, app: &mut App, col: &mut Col, commits: &mut Commits) {
    let (s, e) = (&mut app.sliders, &mut app.editing);
    col.y += 16.0;
    section_title(cx, ui, col, "Sharpness");
    slider(cx, ui, col, s, e, n::clarity, SingleSpec::new("Clarity", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1), commits);
    slider(cx, ui, col, s, e, n::texture, SingleSpec::new("Texture", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1), commits);
    col.y += 16.0;
    section_title(cx, ui, col, "Unsharp Mask");
    let square = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w, col.w));
    let painter = cx.painter(ui, square);
    let mark = cx.fx.mark();
    cx.fx.fill(square, [6.0; 4], Color32::BLACK);
    cx.fx.flush(&painter, mark);
    col.y += col.w + 12.0;
    let mut amount = SingleSpec::new("Amount", "%", 0.0, 0.0, 300.0).decimals(0).steps(1.0, 1.0);
    amount.hard = Some((-100.0, f64::INFINITY));
    slider(cx, ui, col, s, e, n::usmAmount, amount, commits);
    slider(cx, ui, col, s, e, n::usmRadius, SingleSpec::new("Radius", "px", 1.0, 0.5, 10.0).decimals(2).steps(0.01, 0.01), commits);
    slider(cx, ui, col, s, e, n::usmLumaThreshold, SingleSpec::new("Luma Threshold", "%", 0.0, 0.0, 100.0).decimals(1).steps(0.1, 0.1).colours(0x000000, 0xffffff), commits);
    slider(cx, ui, col, s, e, n::usmDetailThreshold, SingleSpec::new("Detail Threshold", "%", 0.0, 0.0, 100.0).decimals(1).steps(0.1, 0.1), commits);
    col.y += 30.0;
}

fn page_light(cx: &mut Cx, ui: &mut Ui, app: &mut App, col: &mut Col, commits: &mut Commits) {
    let rgb = app.is_rgb;
    let mut heights = std::mem::take(&mut app.slide_heights);
    {
        let (s, e) = (&mut app.sliders, &mut app.editing);
        sliding(cx, ui, col, "wb", rgb, &mut heights, "wb", |cx, ui, col| {
            col.y += 16.0;
            section_title(cx, ui, col, "White Balance");
            let mut temp = SingleSpec::new("Temperature", "K", 5500.0, 2700.0, 12000.0).decimals(0).steps(1.0, 10.0).colours(0x3EAFFF, 0xFD8B00);
            temp.scale = widgets::Scale::Reciprocal;
            temp.center = Some(5500.0);
            slider(cx, ui, col, s, e, n::wbTemp, temp, commits);
            slider(cx, ui, col, s, e, n::wbTint, SingleSpec::new("Tint", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colours(0x64FF76, 0xFF66F7), commits);
        });
        col.y += 16.0;
        section_title(cx, ui, col, if rgb { "Light & Colour" } else { "Light" });
        slider(cx, ui, col, s, e, n::exposure, SingleSpec::new("Exposure", "EV", 0.0, -5.0, 5.0).steps(0.0005, 0.01), commits);
        slider(cx, ui, col, s, e, n::contrast, SingleSpec::new("Contrast", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1), commits);
        slider(cx, ui, col, s, e, n::brightness, SingleSpec::new("Brightness", "", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1), commits);
        sliding(cx, ui, col, "vibsat", rgb, &mut heights, "vibsat", |cx, ui, col| {
            separator(cx, ui, col);
            slider(cx, ui, col, s, e, n::vibrance, SingleSpec::new("Vibrance", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colour_end(0xFF0509), commits);
            slider(cx, ui, col, s, e, n::saturation, SingleSpec::new("Saturation", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colour_end(0xFF0509), commits);
        });
        separator(cx, ui, col);
        slider(cx, ui, col, s, e, n::highlights, SingleSpec::new("Highlights", "", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1).colours(0x606060, 0xafafaf), commits);
        slider(cx, ui, col, s, e, n::shadows, SingleSpec::new("Shadows", "", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1).colours(0x303030, 0x3f3f3f), commits);
        slider(cx, ui, col, s, e, n::whites, SingleSpec::new("Whites", "", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1).colours(0xb0b0b0, 0xffffff), commits);
        slider(cx, ui, col, s, e, n::blacks, SingleSpec::new("Blacks", "", 0.0, -100.0, 100.0).decimals(1).steps(0.1, 0.1).colours(0x000000, 0x161616), commits);
    }
    app.slide_heights = heights;
    col.y += 30.0;
}

const SWATCHES: [u32; 8] = [0xf0443a, 0xf58a2e, 0xf2d33a, 0x4cbf4f, 0x3cc6c9, 0x3f6ff0, 0x8b4ef0, 0xe04bc7];

fn mix_hex(c: u32, target: f32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let v = ((c >> shift) & 0xff) as f32;
        ((v + (target - v) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

/// A colour swatch radio button, ringed when selected, dotted when adjusted.
pub fn swatch(cx: &mut Cx, ui: &Ui, id: Id, center: Pos2, colour: Color32, selected: bool, adjusted: bool, disabled: bool) -> bool {
    let rect = Rect::from_center_size(center, vec2(12.0, 12.0));
    let resp = ui.interact(rect.expand(2.0), id, if disabled { Sense::hover() } else { cx.sense(Sense::click()) });
    let ring = cx.anim.flag(id.with("ring"), selected, 0.12, EASE);
    let dim = cx.anim.value(id.with("dim"), if disabled { 0.25 } else { 1.0 }, 0.12, EASE);
    let saved = cx.opacity;
    cx.opacity *= dim;
    let painter = cx.painter(ui, rect);
    let mark = cx.fx.mark();
    cx.fx.fill(rect, [6.0; 4], colour);
    if ring > 0.0 {
        cx.fx.ring(rect.expand(3.5), [9.5; 4], 1.5, fade(cx.pal.color1, ring));
    }
    if adjusted {
        cx.fx.fill(Rect::from_center_size(pos2(center.x, rect.bottom() + 5.5), vec2(3.0, 3.0)), [1.5; 4], cx.pal.color2);
    }
    cx.fx.flush(&painter, mark);
    cx.opacity = saved;
    if resp.hovered() && !disabled && cx.enabled {
        cx.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn page_curves(cx: &mut Cx, ui: &mut Ui, app: &mut App, col: &mut Col, commits: &mut Commits) {
    col.y += 16.0;
    let height = curve_editor::curve_editor(cx, ui, app, col.x, col.y, col.w, commits);
    col.y += height;
    let rgb = app.is_rgb;
    let mixer_identity = app.is_colour_mixer_identity();
    let mut heights = std::mem::take(&mut app.slide_heights);
    let mut band = app.hsl_band;
    {
        let (s, e) = (&mut app.sliders, &mut app.editing);
        sliding(cx, ui, col, "hsl", rgb, &mut heights, "hsl", |cx, ui, col| {
            col.y += 16.0;
            section_title(cx, ui, col, "HSL");
            slider(cx, ui, col, s, e, n::hue, SingleSpec::new("Hue", "º", 0.0, -180.0, 180.0).decimals(1).steps(0.01, 0.1).colour_end(0xFF0509), commits);
            slider(cx, ui, col, s, e, n::vibrance, SingleSpec::new("Vibrance", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colour_end(0xFF0509), commits);
            slider(cx, ui, col, s, e, n::saturation, SingleSpec::new("Saturation", "%", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colour_end(0xFF0509), commits);
        });
        sliding(cx, ui, col, "mixer", rgb, &mut heights, "mixer", |cx, ui, col| {
            col.y += 16.0;
            // The title shares its row with the mixer's own reset.
            let row = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w - 6.0, 28.0));
            let painter = cx.painter(ui, row);
            shadow_text(&painter, pos2(col.x, row.center().y), Align2::LEFT_CENTER, "Colour Mixer", font(SECTION_TITLE, Weight::SemiBold), cx.pal.color1, 0.35);
            if revert_button(cx, ui, Id::new("mixer-reset"), pos2(row.right() - 28.0, row.top()), !mixer_identity) {
                commits.reset_mixer = true;
            }
            col.y += 28.0 + 6.0;
            // The swatches, centred over the sliders.
            let pitch = 12.0 + 8.0;
            let total = pitch * 8.0 - 8.0;
            let left = col.x + (col.w - 6.0 - total) / 2.0;
            let cy = col.y + 8.0 + 6.0;
            for (i, &c) in SWATCHES.iter().enumerate() {
                let adjusted = (0..3).any(|ch| s.v[hsl_key(i, ch)].abs() > SLIDER_TOLERANCE);
                if swatch(cx, ui, Id::new(("band", i)), pos2(left + pitch * i as f32 + 6.0, cy), hex(c), band == i, adjusted, false) {
                    band = i;
                }
            }
            col.y += 8.0 + 12.0 + 10.0;
            let colour = SWATCHES[band];
            let prev = SWATCHES[(band + 7) % 8];
            let next = SWATCHES[(band + 1) % 8];
            let ends = [(prev, next), (0x4d4d4d, colour), (mix_hex(colour, 0.0, 0.6), mix_hex(colour, 255.0, 0.5))];
            for (ch, name) in ["Hue", "Saturation", "Luminance"].iter().enumerate() {
                let spec = SingleSpec::new(name, "", 0.0, -100.0, 100.0).decimals(1).steps(0.01, 0.1).colours(ends[ch].0, ends[ch].1);
                let index = hsl_key(band, ch);
                let id = Id::new(("hsl", ch));
                if let Some((old, new)) = single_group(cx, ui, id, col.x, col.y, col.w, &mut s.v[index], &spec, e) {
                    commits.sliders.push((index, old, new));
                }
                col.y += widgets::SINGLE_HEIGHT;
            }
        });
    }
    app.hsl_band = band;
    app.slide_heights = heights;
    col.y += 30.0;
}

/// The darkroom's static histogram of one input channel, filled in its colour.
fn static_histogram(cx: &mut Cx, ui: &Ui, rect: Rect, data: Option<&[u32; 256]>, colour: Color32) {
    let painter = cx.painter(ui, rect);
    let mark = cx.fx.mark();
    cx.fx.fill(rect, [8.0, 8.0, 4.0, 4.0], cx.pal.bg1);
    cx.fx.flush(&painter, mark);
    let Some(data) = data else { return };
    let max = *data.iter().max().unwrap_or(&0);
    if max == 0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let colour = fade(colour, painter.opacity());
    for (i, &v) in data.iter().enumerate() {
        let x = rect.left() + i as f32 / 255.0 * rect.width();
        let y = rect.bottom() - v as f32 / max as f32 * rect.height();
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(pos2(x, y), colour);
        mesh.colored_vertex(pos2(x, rect.bottom()), colour);
        if i > 0 {
            mesh.add_triangle(base - 2, base - 1, base);
            mesh.add_triangle(base - 1, base, base + 1);
        }
    }
    painter.with_clip_rect(rect).add(mesh);
}

/// The readouts under a multi-pointer slider, at the left, centre and right.
#[allow(clippy::too_many_arguments)]
fn readouts(cx: &mut Cx, ui: &mut Ui, id: Id, x: f32, y: f32, w: f32, values: &mut [(&mut f64, widgets::Precision<'_>, egui::Align)], editing: &mut Editing) {
    // Spans 102% of the column, pulled 6px left.
    let (x, w) = (x - 6.0, w * 1.02);
    let third = w / values.len() as f32;
    for (k, (v, p, align)) in values.iter_mut().enumerate() {
        let size = widgets::precision_size(&cx.ctx, &p.format(**v));
        let cell = Rect::from_min_size(pos2(x + third * k as f32, y), vec2(third, size.y));
        let left = match align {
            egui::Align::Min => cell.left(),
            egui::Align::Center => cell.center().x - size.x / 2.0,
            egui::Align::Max => cell.right() - size.x,
        };
        widgets::precision(cx, ui, id.with(k), Rect::from_min_size(pos2(left, y), size), v, *p, editing);
    }
}

fn page_darkroom(cx: &mut Cx, ui: &mut Ui, app: &mut App, col: &mut Col) {
    col.y += 16.0;
    let hist = app.input_histograms;
    let names = ["Red Input", "Green Input", "Blue Input"];
    let colours = [Color32::from_rgb(255, 0, 0), Color32::from_rgb(0, 255, 0), Color32::from_rgb(0, 0, 255)];
    let keys = [
        (n::redBlackPoint, n::redGamma, n::redWhitePoint),
        (n::greenBlackPoint, n::greenGamma, n::greenWhitePoint),
        (n::blueBlackPoint, n::blueGamma, n::blueWhitePoint),
    ];
    let inner = hex(0x430000);
    for c in 0..3 {
        let id = Id::new(("triple", c));
        let painter = cx.painter(ui, Rect::from_min_size(pos2(col.x, col.y), vec2(col.w, 24.0)));
        shadow_text(&painter, pos2(col.x, col.y + 12.0), Align2::LEFT_CENTER, names[c], font(12.0, Weight::Regular), cx.pal.color2, 0.35);
        col.y += 24.0;
        let h = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w, col.w / 4.0));
        static_histogram(cx, ui, h, hist.as_ref().map(|b| &b[c]), colours[c]);
        col.y += col.w / 4.0 + 4.0;
        let (ka, kb, kc) = keys[c];
        let track = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w - 6.0, 22.0));
        let v = &mut app.sliders.v;
        let (mut a, mut b, mut cc) = (v[ka], v[kb], v[kc]);
        widgets::multi_slider(cx, ui, id, track, &mut a, Some((&mut b, 0.1, 2.0, 1.0)), &mut cc, (0.0, 255.0), 0.0, 255.0, inner, Color32::BLACK);
        col.y += 22.0;
        let p = |d: f64, min: f64, max: f64, decimals: usize, step: f64| widgets::Precision { default: d, min, max, unit: "", step, keyboard_step: 0.1, decimals };
        readouts(
            cx,
            ui,
            id.with("values"),
            col.x,
            col.y,
            col.w,
            &mut [
                (&mut a, p(0.0, 0.0, 255.0, 1, 0.01), egui::Align::Min),
                (&mut b, p(1.0, 0.1, 2.0, 3, 0.0001), egui::Align::Center),
                (&mut cc, p(255.0, 0.0, 255.0, 1, 0.01), egui::Align::Max),
            ],
            &mut app.editing,
        );
        let v = &mut app.sliders.v;
        (v[ka], v[kb], v[kc]) = (a, b, cc);
        col.y += widgets::VALUE_FONT + widgets::VALUE_PAD * 2.0 + 4.0;
    }
    // RGB Output: two pointers, no histogram, pulled up like a single group.
    col.y -= 3.0;
    let painter = cx.painter(ui, Rect::from_min_size(pos2(col.x, col.y), vec2(col.w, 24.0)));
    shadow_text(&painter, pos2(col.x, col.y + 12.0), Align2::LEFT_CENTER, "RGB Output", font(12.0, Weight::Regular), cx.pal.color2, 0.35);
    col.y += 24.0;
    let track = Rect::from_min_size(pos2(col.x, col.y), vec2(col.w - 6.0, 22.0));
    let v = &mut app.sliders.v;
    let (mut a, mut c) = (v[n::rgbOutputBlack], v[n::rgbOutputWhite]);
    widgets::multi_slider(cx, ui, Id::new("output"), track, &mut a, None, &mut c, (0.0, 255.0), 0.0, 255.0, inner, Color32::BLACK);
    col.y += 22.0;
    let p = |d: f64| widgets::Precision { default: d, min: 0.0, max: 255.0, unit: "", step: 0.01, keyboard_step: 0.1, decimals: 1 };
    readouts(cx, ui, Id::new("output-values"), col.x, col.y, col.w, &mut [(&mut a, p(0.0), egui::Align::Min), (&mut c, p(255.0), egui::Align::Max)], &mut app.editing);
    app.sliders.v[n::rgbOutputBlack] = a;
    app.sliders.v[n::rgbOutputWhite] = c;
    col.y += widgets::VALUE_FONT + widgets::VALUE_PAD * 2.0;
    col.y += 30.0;
}

// ── Buttons ───────────────────────────────────────────────────────────────────

const REVERT_ARC: &str = "M12.7 7.56A4.5 4.5 0 1 1 7.57 12.78";
const REVERT_HEAD: &str = "M9.15 7.5 12 5.35 12 9.65Z";

/// The reset arrow, which stays mounted and fades so its row never moves.
pub fn revert_button(cx: &mut Cx, ui: &Ui, id: Id, at: Pos2, visible: bool) -> bool {
    let shown = cx.anim.flag(id.with("shown"), visible, 0.5, EASE);
    if shown <= 0.0 {
        return false;
    }
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= shown;
    cx.enabled &= visible;
    let ink = cx.pal.color2;
    let resp = glass_button(cx, ui, id, Rect::from_min_size(at, vec2(28.0, 28.0)), false, false, |p, r, s| {
        let placed = Placed::new(r.min + (r.size() - vec2(28.0, 28.0) * s) / 2.0, 28.0 / 24.0 * s);
        placed.stroke(p, REVERT_ARC, egui::Stroke::new(1.2_f32 * 28.0 / 24.0 * s, ink));
        placed.fill(p, REVERT_HEAD, ink);
    });
    (cx.opacity, cx.enabled) = saved;
    resp.clicked() && visible
}

// ── The side bar ──────────────────────────────────────────────────────────────

pub fn sidebar(app: &mut App, cx: &mut Cx, ui: &mut Ui, geo: &super::geo::Geo) {
    let inactive = app.sidebar_inactive();
    let ctx = cx.ctx.clone();

    // Histogram panel.
    let hp = geo.histogram;
    block(ui, Id::new("histogram-panel"), hp);
    let panel_dim = cx.anim.value("hist-panel-dim", if app.debug_tools && !app.debug.histogram { 0.25 } else { 1.0 }, 0.2, EASE);
    let saved = cx.opacity;
    cx.opacity *= panel_dim;
    let painter = cx.painter(ui, hp);
    panel(cx, &painter, hp, false);
    let inner_dim = cx.anim.value("hist-dim", if inactive { 0.25 } else { 1.0 }, 0.2, EASE);
    cx.opacity *= inner_dim;
    let plot = hp.shrink(5.0);
    let painter = cx.painter(ui, plot);
    let mark = cx.fx.mark();
    cx.fx.fill(plot, [7.0; 4], cx.pal.bg1);
    cx.fx.flush(&painter, mark);
    app.hist_size = (plot.width() * cx.ppp, plot.height() * cx.ppp);
    if let Some(id) = app.hist_plot.id {
        if app.image.is_some() {
            let shape = egui::epaint::RectShape::filled(plot, 7.0, fade(Color32::WHITE, painter.opacity())).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
            painter.add(shape);
        }
    }
    cx.opacity = saved;

    // Tools panel.
    let tp = geo.tools;
    block(ui, Id::new("tools-panel"), tp);
    let painter = cx.painter(ui, tp);
    panel(cx, &painter, tp, true);

    let quick_dim = cx.anim.value("quick-dim", if app.export.menu_open || inactive { 0.25 } else { 1.0 }, 0.2, EASE);
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= quick_dim;
    cx.enabled &= !inactive;
    let row = Rect::from_min_size(pos2(tp.left() + 12.0, tp.top() + 10.0), vec2(tp.width() - 24.0, 28.0));
    // The colour mode toggle reads inverted: the thumb travels when RGB is off.
    let resp = glass_toggle(cx, ui, Id::new("colour-mode"), row.min, 54.0, !app.is_rgb, Stop { x: 3.0, w: 24.0 }, Stop { x: 27.0, w: 24.0 }, |p, o, s| {
        let at = |x: f32, y: f32| o + vec2(x, y) * s;
        // Screen-blended primaries: drawn additively, they sum to white.
        let add = |c: [u8; 3]| Color32::from_rgba_premultiplied(c[0], c[1], c[2], 0);
        p.circle_filled(at(15.0, 12.0), 4.0 * s, add([255, 0, 0]));
        p.circle_filled(at(13.0, 16.0), 4.0 * s, add([0, 255, 0]));
        p.circle_filled(at(17.0, 16.0), 4.0 * s, add([0, 0, 255]));
        p.circle_filled(at(39.0, 14.0), 6.0 * s, Color32::WHITE);
    });
    if resp.clicked() {
        app.toggle_colour_mode();
    }
    let invert_shown = cx.anim.flag("invert-slot", app.is_darkroom, 0.25, EASE);
    if invert_shown > 0.0 {
        let s2 = (cx.opacity, cx.enabled);
        cx.opacity *= invert_shown;
        cx.enabled &= app.is_darkroom;
        let resp = glass_toggle(cx, ui, Id::new("invert"), row.min + vec2(62.0, 0.0), 54.0, app.sliders.invert, Stop { x: 3.0, w: 24.0 }, Stop { x: 27.0, w: 24.0 }, |p, o, s| {
            p.circle_filled(o + vec2(15.0, 14.0) * s, 6.0 * s, Color32::WHITE);
            p.circle_stroke(o + vec2(39.0, 14.0) * s, 6.0 * s, egui::Stroke::new(2.0_f32 * s, Color32::WHITE));
        });
        if resp.clicked() {
            app.toggle_invert();
        }
        (cx.opacity, cx.enabled) = s2;
    }
    if revert_button(cx, ui, Id::new("revert"), pos2(row.right() - 28.0, row.top()), !app.is_identity()) {
        app.reset_adjustments();
    }
    (cx.opacity, cx.enabled) = saved;

    // The pager.
    let pager = Rect::from_min_max(pos2(tp.left(), tp.top() + 38.0), tp.max);
    let controls_dim = quick_dim;
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= controls_dim;
    cx.enabled &= !inactive;
    pager_view(app, cx, ui, pager, &ctx);
    (cx.opacity, cx.enabled) = saved;

    // Footer.
    footer(app, cx, ui, geo.footer, inactive);
}

fn pager_view(app: &mut App, cx: &mut Cx, ui: &mut Ui, pager: Rect, ctx: &egui::Context) {
    let w = pager.width();
    let now = cx.now();
    // The two pagers sit side by side on an imaginary strip, and switching
    // modes slides the strip across.
    let slide = cx.anim.value("mode-slide", if app.is_darkroom { 1.0 } else { 0.0 }, 0.5, LINEAR);
    let slide = super::anim::cubic_in_out(slide);

    // Paging: smooth scrolls between snapped pages, and a trackpad's sideways
    // swipe or a shift-wheel moves the strip directly until it settles.
    let hovered = ui.rect_contains_pointer(pager) && cx.enabled;
    let (scroll, side_back, side_fwd) = ctx.input(|i| {
        (
            i.smooth_scroll_delta,
            i.pointer.button_pressed(egui::PointerButton::Extra1),
            i.pointer.button_pressed(egui::PointerButton::Extra2),
        )
    });
    if hovered && (side_back || side_fwd) {
        app.move_page(side_fwd);
    }
    let offset_id = Id::new("pager-offset");
    if hovered && !app.is_darkroom && scroll.x.abs() > scroll.y.abs() && scroll.x != 0.0 {
        let next = (app.pager_offset - scroll.x / w).clamp(0.0, 2.0);
        app.pager_offset = next;
        cx.anim.set(offset_id, next);
        app.pager_idle_at = now + 0.15;
        app.page_normal = next.round() as usize;
    }
    let offset = if now < app.pager_idle_at {
        app.pager_offset
    } else {
        cx.anim.value(offset_id, app.page_normal as f32, 0.35, EASE_IN_OUT)
    };
    app.pager_offset = offset;
    if now < app.pager_idle_at {
        ctx.request_repaint();
    }

    let mut commits = Commits::default();
    let mut child = ui.new_child(UiBuilder::new().max_rect(pager));
    child.set_clip_rect(pager.intersect(ui.clip_rect()));
    let side_mask = Mask { rect: pager, top: 0.0, bottom: 0.0, left: 16.0, right: 12.0 };
    let page_mask = |r: Rect| Mask { rect: r, top: 20.0, bottom: 24.0, left: 0.0, right: 0.0 };

    let draw_page = |app: &mut App, cx: &mut Cx, child: &mut Ui, index: usize, x: f32, commits: &mut Commits| {
        let page_rect = Rect::from_min_size(pos2(x, pager.top()), pager.size());
        if page_rect.intersect(pager).width() < 0.5 {
            return;
        }
        cx.salt = index;
        let scroll = app.page_scroll[index];
        let mut col = Col { x: x + 16.0, y: pager.top() - scroll, w: pager.width() - 28.0 };
        let start = col.y;
        let mut page_ui = child.new_child(UiBuilder::new().max_rect(page_rect));
        page_ui.set_clip_rect(page_rect.intersect(child.clip_rect()));
        cx.masks.push(side_mask);
        cx.masks.push(page_mask(page_rect));
        match index {
            0 => page_sharpness(cx, &mut page_ui, app, &mut col, commits),
            1 => page_light(cx, &mut page_ui, app, &mut col, commits),
            2 => page_curves(cx, &mut page_ui, app, &mut col, commits),
            _ => page_darkroom(cx, &mut page_ui, app, &mut col),
        }
        cx.masks.pop();
        cx.masks.pop();
        let content = col.y - start;
        let max_scroll = (content - pager.height()).max(0.0);
        // Vertical intent scrolls the page under the pointer.
        if page_rect.intersect(pager).contains(ctx.input(|i| i.pointer.hover_pos()).unwrap_or(Pos2::ZERO)) && cx.enabled {
            let dy = ctx.input(|i| i.smooth_scroll_delta);
            if dy.y.abs() > dy.x.abs() {
                app.page_scroll[index] = (app.page_scroll[index] - dy.y).clamp(0.0, max_scroll);
            }
        }
        app.page_scroll[index] = app.page_scroll[index].min(max_scroll);
    };

    if slide < 1.0 {
        let base = pager.left() - slide * w;
        for i in 0..3 {
            draw_page(app, cx, &mut child, i, base + (i as f32 - offset) * w, &mut commits);
        }
    }
    if slide > 0.0 {
        draw_page(app, cx, &mut child, 3, pager.left() + (1.0 - slide) * w, &mut commits);
    }

    for (index, old, new) in commits.sliders {
        app.commit_slider(index, old, new);
    }
    if let Some((old, new)) = commits.curves {
        app.commit(crate::model::history::Action::Curves { old, new });
    }
    if commits.reset_mixer {
        app.reset_colour_mixer();
    }
}

fn footer(app: &mut App, cx: &mut Cx, ui: &mut Ui, f: Rect, inactive: bool) {
    block(ui, Id::new("footer"), f);
    let painter = cx.painter(ui, f);
    panel(cx, &painter, f, false);
    let dim = cx.anim.value("footer-dim", if inactive { 0.25 } else { 1.0 }, 0.2, EASE);
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= dim;
    cx.enabled &= !inactive;

    // Export, welded to the arrow that opens its settings.
    let ex = Rect::from_min_size(pos2(f.left() + 10.0, f.center().y - 14.0), vec2(63.0, 28.0));
    let arrow = Rect::from_min_size(pos2(ex.right() + 1.0, ex.top()), vec2(27.0, 28.0));
    let painter = cx.painter(ui, ex);
    let mark = cx.fx.mark();
    cx.fx.fill(ex, [6.0, 3.0, 3.0, 6.0], cx.pal.bg2);
    cx.fx.fill(arrow, [3.0, 6.0, 6.0, 3.0], cx.pal.bg2);
    cx.fx.flush(&painter, mark);
    let ex_resp = ui.interact(ex, Id::new("export"), if app.export.pending { Sense::hover() } else { cx.sense(Sense::click()) });
    let label_alpha = if app.export.pending { 0.6 } else { 1.0 };
    let label_alpha = cx.anim.value("export-label", label_alpha, 0.2, EASE);
    painter.text(ex.center(), Align2::CENTER_CENTER, "Export", font(16.0, Weight::Medium), fade(cx.pal.color2, label_alpha));
    if ex_resp.clicked() {
        app.export_requested = true;
    }
    let arrow_resp = ui.interact(arrow, Id::new("export-arrow"), cx.sense(Sense::click()));
    if arrow_resp.clicked() {
        app.export.menu_open = !app.export.menu_open;
        app.export.menu_toggled = true;
    }
    if ex_resp.hovered() || arrow_resp.hovered() {
        cx.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let turn = cx.anim.value("export-arrow-turn", if app.export.menu_open { 1.0 } else { 0.0 }, 0.2, EASE);
    let chevron = |t: f32| {
        // Up when closed, turned half a revolution when open.
        let angle = std::f32::consts::PI * t;
        let (s, c) = angle.sin_cos();
        let center = arrow.center();
        [pos2(-5.5, 3.0), pos2(0.0, -3.0), pos2(5.5, 3.0)].map(|p| center + vec2(p.x * c - p.y * s, p.x * s + p.y * c))
    };
    let pts = chevron(turn);
    let stroke = egui::Stroke::new(1.6_f32, cx.pal.color2);
    super::path::capped_line(&painter, pts[0], pts[1], 1.6, cx.pal.color2);
    super::path::capped_line(&painter, pts[1], pts[2], 1.6, cx.pal.color2);
    let _ = stroke;

    // Page dots: both sets stay mounted and crossfade.
    let dots_center = f.center();
    let normal = cx.anim.flag("dots-normal", !app.is_darkroom, 0.5, EASE);
    let dark = 1.0 - normal;
    let pitch = 6.0 + 12.0;
    for (set_alpha, count, active) in [(normal, 3usize, app.page_normal), (dark, 1, 0)] {
        if set_alpha <= 0.0 {
            continue;
        }
        let left = dots_center.x - (pitch * count as f32 - 12.0) / 2.0 + 3.0;
        for i in 0..count {
            let c = pos2(left + pitch * i as f32, dots_center.y);
            let id = Id::new(("dot", count, i));
            let on = cx.anim.flag(id, active == i, 0.2, EASE);
            let r = 3.0 * (1.0 + 0.3 * on);
            let saved_o = cx.opacity;
            cx.opacity *= set_alpha;
            let painter = cx.painter(ui, Rect::from_center_size(c, vec2(6.0, 6.0)));
            let mark = cx.fx.mark();
            cx.fx.fill(Rect::from_center_size(c, vec2(r * 2.0, r * 2.0)), [r; 4], super::theme::mix(cx.pal.color5, cx.pal.color1, on));
            cx.fx.flush(&painter, mark);
            cx.opacity = saved_o;
            if count == 3 && set_alpha > 0.5 {
                let resp = ui.interact(Rect::from_center_size(c, vec2(12.0, 12.0)), id.with("hit"), cx.sense(Sense::click()));
                if resp.clicked() {
                    app.scroll_to_page(i);
                }
                if resp.hovered() {
                    cx.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        }
    }

    // The mode toggle.
    let sun = app.icons.sun.id();
    let bulb = app.icons.bulb.id();
    let origin = pos2(f.right() - 10.0 - 53.0, f.center().y - 14.0);
    let resp = glass_toggle(cx, ui, Id::new("mode"), origin, 53.0, app.is_darkroom, Stop { x: 3.0, w: 24.0 }, Stop { x: 26.0, w: 24.0 }, |p, o, s| {
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        let tint = fade(Color32::WHITE, p.opacity());
        p.image(sun, Rect::from_min_size(o + vec2(7.0, 6.0) * s, vec2(16.0, 16.0) * s), uv, tint);
        p.image(bulb, Rect::from_min_size(o + vec2(53.0 - 5.0 - 20.0, 4.0) * s, vec2(20.0, 20.0) * s), uv, tint);
    });
    if resp.clicked() {
        app.toggle_mode();
    }
    (cx.opacity, cx.enabled) = saved;
}
