// src-native/src/ui/overlays.rs
//
// Everything that opens over the app: the export settings menu and progress
// modal, and the settings window.

use std::sync::atomic::Ordering;

use egui::{pos2, vec2, Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, Ui, UiBuilder};

use super::anim::{EASE, EASE_OUT_EMPH};
use super::cx::{text_width, Cx};
use super::path::Placed;
use super::theme::{black, fade, font, mix, outfit, white, Weight};
use super::widgets::{glass_button, glass_toggle, int_slider, pill, Stop};
use crate::app::{default_output_space, App};
use crate::export_encoding::ExportFormat;
use crate::gpu::chrome::{shadow, GlassStyle};
use crate::model::shortcuts::{actions_in, display, GROUPS};
use crate::output_space::OutputSpace;

pub const SPACE_LABELS: [&str; 4] = ["sRGB", "Display P3", "Adobe RGB", "Rec.2020"];

fn format_key(f: ExportFormat) -> u8 {
    match f {
        ExportFormat::Png => 0,
        ExportFormat::Jpeg => 1,
        ExportFormat::Webp => 2,
        ExportFormat::Tiff => 3,
    }
}

// ── Export settings menu ──────────────────────────────────────────────────────

/// A labelled setting: label and value on a row, then the control under it.
fn header(cx: &mut Cx, painter: &egui::Painter, x0: f32, x1: f32, y: f32, label: &str, value: Option<String>) {
    painter.text(pos2(x0, y + 12.0), Align2::LEFT_CENTER, label, font(12.0, Weight::Medium), cx.pal.color2);
    if let Some(v) = value {
        painter.text(pos2(x1, y + 12.0), Align2::RIGHT_CENTER, v, font(12.0, Weight::Regular), cx.pal.color3);
    }
}

fn hints(cx: &mut Cx, painter: &egui::Painter, x0: f32, x1: f32, y: f32, left: &str, right: &str) {
    painter.text(pos2(x0, y + 12.0), Align2::LEFT_CENTER, left, font(10.0, Weight::Regular), cx.pal.color3);
    painter.text(pos2(x1, y + 12.0), Align2::RIGHT_CENTER, right, font(10.0, Weight::Regular), cx.pal.color3);
}

/// A pill row, equally split.
fn pills(cx: &mut Cx, ui: &Ui, id: Id, x0: f32, x1: f32, y: f32, labels: &[&str], active: usize) -> Option<usize> {
    let n = labels.len() as f32;
    let w = (x1 - x0 - 8.0 * (n - 1.0)) / n;
    let mut picked = None;
    for (i, label) in labels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x0 + i as f32 * (w + 8.0), y), vec2(w, 34.0));
        if pill(cx, ui, id.with(i), r, label, i == active).clicked() {
            picked = Some(i);
        }
    }
    picked
}

fn menu_height(app: &App) -> f32 {
    let s = &app.export.settings;
    let setting = match s.format {
        ExportFormat::Png | ExportFormat::Jpeg => 74.0,
        ExportFormat::Webp => 34.0 + 12.0 + 74.0,
        ExportFormat::Tiff => 34.0,
    };
    12.0 + setting + 12.0 + 1.0 + 12.0 + 106.0 + 12.0 + 34.0 + 12.0 + 1.0 + 12.0 + 34.0 + 12.0
}

pub fn export_menu(app: &mut App, cx: &mut Cx, ui: &mut Ui, footer: Rect) {
    let t = cx.anim.flag("export-menu", app.export.menu_open, 0.15, EASE_OUT_EMPH);
    if !app.export.menu_open {
        cx.anim.set("export-menu", 0.0);
        return;
    }
    // A change of format re-picks the space, so 8-bit lands on sRGB and a
    // 16-bit TIFF on the working space.
    let key = (app.export.settings.format, app.export.settings.tiff_bit_depth);
    if (format_key(key.0), key.1) != (format_key(app.export.defaulted_for.0), app.export.defaulted_for.1) {
        app.export.defaulted_for = key;
        app.export.settings.output_space = default_output_space(key.0, key.1);
    }
    let height = menu_height(app);
    let menu = Rect::from_min_max(pos2(footer.left() + 8.0, footer.top() - 22.0 - height), pos2(footer.right() - 16.0, footer.top() - 22.0));
    let scale = 0.95 + 0.05 * t;
    let shown = Rect::from_center_size(menu.center() + vec2(0.0, 50.0 * (1.0 - t)), menu.size() * scale);
    let saved = cx.opacity;
    cx.opacity *= t;
    let painter = cx.painter(ui, shown);
    let mark = cx.fx.mark();
    cx.fx.shadow(shown, 8.0, &shadow(0.0, 8.0, 24.0, 0.0, black(0.6)));
    cx.fx.fill(shown, [8.0; 4], cx.pal.bg3);
    cx.fx.flush(&painter, mark);
    super::sidebar::block(ui, Id::new("export-menu-block"), shown);

    // Laid out at full size and scaled with the entrance.
    let mut child = ui.new_child(UiBuilder::new().max_rect(shown));
    child.set_clip_rect(shown.expand(2.0));
    let (x0, x1) = (menu.left() + 12.0, menu.right() - 12.0);
    let offset = shown.min - menu.min;
    let mut y = menu.top() + 12.0;
    let s = &mut app.export.settings;
    let id = Id::new("export-menu");
    let p = cx.painter(&child, shown);
    let p = p.with_clip_rect(shown);
    let at = |y: f32| y + offset.y;
    match s.format {
        ExportFormat::Png => {
            header(cx, &p, x0, x1, at(y), "Compression", Some(s.png_compression.to_string()));
            int_slider(cx, &child, id.with("png"), Rect::from_min_size(pos2(x0, at(y + 30.0)), vec2(x1 - x0, 22.0)), &mut s.png_compression, 6, 0.0, 9.0, true);
            hints(cx, &p, x0, x1, at(y + 50.0), "Fast", "Small");
            y += 74.0;
        }
        ExportFormat::Jpeg => {
            header(cx, &p, x0, x1, at(y), "Quality", Some(s.jpeg_quality.to_string()));
            int_slider(cx, &child, id.with("jpeg"), Rect::from_min_size(pos2(x0, at(y + 30.0)), vec2(x1 - x0, 22.0)), &mut s.jpeg_quality, 90, 1.0, 100.0, false);
            hints(cx, &p, x0, x1, at(y + 50.0), "Low", "High");
            y += 74.0;
        }
        ExportFormat::Webp => {
            if let Some(i) = pills(cx, &child, id.with("webp-mode"), x0, x1, at(y), &["Lossless", "Lossy"], if s.webp_lossless { 0 } else { 1 }) {
                s.webp_lossless = i == 0;
            }
            y += 34.0 + 12.0;
            if s.webp_lossless {
                header(cx, &p, x0, x1, at(y), "Compression", Some(s.webp_compression.to_string()));
                int_slider(cx, &child, id.with("webp-c"), Rect::from_min_size(pos2(x0, at(y + 30.0)), vec2(x1 - x0, 22.0)), &mut s.webp_compression, 6, 0.0, 9.0, true);
                hints(cx, &p, x0, x1, at(y + 50.0), "Fast", "Small");
            } else {
                header(cx, &p, x0, x1, at(y), "Quality", Some(s.webp_quality.to_string()));
                int_slider(cx, &child, id.with("webp-q"), Rect::from_min_size(pos2(x0, at(y + 30.0)), vec2(x1 - x0, 22.0)), &mut s.webp_quality, 90, 1.0, 100.0, false);
                hints(cx, &p, x0, x1, at(y + 50.0), "Low", "High");
            }
            y += 74.0;
        }
        ExportFormat::Tiff => {
            let kind = if app.is_rgb { "RGB" } else { "Greyscale" };
            let labels = [format!("{kind} 8-bit"), format!("{kind} 16-bit")];
            let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
            if let Some(i) = pills(cx, &child, id.with("depth"), x0, x1, at(y), &labels, if s.tiff_bit_depth == 16 { 1 } else { 0 }) {
                s.tiff_bit_depth = if i == 1 { 16 } else { 8 };
            }
            y += 34.0;
        }
    }
    y += 12.0;
    p.rect_filled(Rect::from_min_size(pos2(x0, at(y)), vec2(x1 - x0, 1.0)), 0.0, cx.pal.bg5);
    y += 1.0 + 12.0;
    header(cx, &p, x0, x1, at(y), "Colour space", None);
    y += 30.0;
    let w = (x1 - x0 - 8.0) / 2.0;
    for (i, space) in OutputSpace::ALL.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x0 + (i % 2) as f32 * (w + 8.0), at(y + (i / 2) as f32 * 42.0)), vec2(w, 34.0));
        if pill(cx, &child, id.with(("space", i)), r, SPACE_LABELS[i], s.output_space == *space).clicked() {
            s.output_space = *space;
        }
    }
    y += 76.0 + 12.0;
    p.text(pos2(x0, at(y + 17.0)), Align2::LEFT_CENTER, "Soft proof", font(12.0, Weight::Medium), cx.pal.color2);
    let label = if app.proofing { "On" } else { "Off" };
    let pw = text_width(&cx.ctx, label, font(13.0, Weight::SemiBold)) + 24.0;
    if pill(cx, &child, id.with("proof"), Rect::from_min_size(pos2(x1 - pw, at(y)), vec2(pw, 34.0)), label, app.proofing).clicked() {
        app.proofing = !app.proofing;
    }
    let s = &mut app.export.settings;
    y += 34.0 + 12.0;
    p.rect_filled(Rect::from_min_size(pos2(x0, at(y)), vec2(x1 - x0, 1.0)), 0.0, cx.pal.bg5);
    y += 1.0 + 12.0;
    let formats = [ExportFormat::Webp, ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Tiff];
    let active = formats.iter().position(|f| format_key(*f) == format_key(s.format)).unwrap_or(1);
    if let Some(i) = pills(cx, &child, id.with("format"), x0, x1, at(y), &["WebP", "PNG", "JPEG", "TIFF"], active) {
        s.format = formats[i];
    }
    cx.opacity = saved;

    // A click outside the footer and the menu closes it.
    let clicked = cx.ctx.input(|i| i.pointer.any_click());
    let pos = cx.ctx.input(|i| i.pointer.interact_pos());
    if clicked && !app.export.menu_toggled && pos.is_some_and(|p| !footer.contains(p) && !menu.contains(p)) {
        app.export.menu_open = false;
    }
}

// ── Export progress ───────────────────────────────────────────────────────────

pub fn export_modal(app: &mut App, cx: &mut Cx, ui: &mut Ui) {
    if !app.export.modal {
        return;
    }
    let screen = cx.ctx.screen_rect();
    ui.interact(screen, Id::new("export-modal-block"), Sense::click_and_drag());
    let painter = cx.painter(ui, screen);
    let mark = cx.fx.mark();
    cx.fx.fill(screen, [0.0; 4], black(0.6));
    let progress = f32::from_bits(app.export.progress.load(Ordering::SeqCst)).clamp(0.0, 1.0);
    let modal = Rect::from_center_size(screen.center(), vec2(260.0 + 72.0, 28.0 * 2.0 + 24.0 + 14.0 + 4.0));
    cx.fx.shadow(modal, 10.0, &shadow(0.0, 12.0, 40.0, 0.0, black(0.7)));
    cx.fx.fill(modal, [10.0; 4], cx.pal.bg3);
    let track = Rect::from_min_size(pos2(modal.left() + 36.0, modal.bottom() - 28.0 - 4.0), vec2(modal.width() - 72.0, 4.0));
    cx.fx.fill(track, [2.0; 4], cx.pal.bg5);
    let shown = cx.anim.value("export-progress", progress, 0.15, super::anim::Ease(0.0, 0.0, 0.58, 1.0));
    if shown > 0.0 {
        cx.fx.fill(Rect::from_min_size(track.min, vec2(track.width() * shown, 4.0)), [2.0; 4], cx.pal.color1);
    }
    cx.fx.flush(&painter, mark);
    painter.text(
        pos2(modal.center().x, modal.top() + 28.0 + 12.0),
        Align2::CENTER_CENTER,
        format!("Exporting… {}%", (progress * 100.0).round()),
        font(15.0, Weight::Medium),
        cx.pal.color2,
    );
    cx.ctx.request_repaint();
}

// ── Settings ──────────────────────────────────────────────────────────────────

const SECTIONS: [(&str, &str); 2] = [
    ("Shortcuts", "M3 7h18v10H3zM7 11h.01M11 11h.01M15 11h.01M8.5 15h7"),
    ("Debug", "M4 13h3l2.5-6 4 12 2.5-6h4"),
];

/// One row in a settings table: a label and description, and a value or control.
struct Row<'a> {
    label: &'a str,
    description: &'a str,
    value: Option<String>,
}

fn wrap(ctx: &egui::Context, text: &str, f: egui::FontId, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    ctx.fonts(|fonts| fonts.layout(text.to_string(), f, color, width))
}

/// A titled table of rows; returns its height. `control` draws a row's
/// right-hand control, given its rect, and returns nothing.
#[allow(clippy::too_many_arguments)]
fn group(
    cx: &mut Cx,
    ui: &Ui,
    x: f32,
    y: f32,
    w: f32,
    title: Option<&str>,
    note: Option<&str>,
    rows: &[Row<'_>],
    mut control: impl FnMut(&mut Cx, &Ui, usize, Pos2) -> f32,
) -> f32 {
    let mut cursor = y;
    let painter = cx.painter(ui, Rect::from_min_size(pos2(x, y), vec2(w, 24.0)));
    if let Some(t) = title {
        painter.text(pos2(x + 12.0, cursor + 12.0), Align2::LEFT_CENTER, t, font(15.0, Weight::SemiBold), cx.pal.color1);
        cursor += 24.0 + 4.0;
    }
    // Lay the rows out first, to size the table they share.
    let mut heights = Vec::new();
    let mut galleys = Vec::new();
    for row in rows {
        let label = wrap(&cx.ctx, row.label, font(13.0, Weight::Regular), cx.pal.color2, w * 0.5);
        let desc = (!row.description.is_empty()).then(|| wrap(&cx.ctx, row.description, font(11.5, Weight::Regular), cx.pal.color3, w * 0.55));
        let value = row.value.as_ref().map(|v| wrap(&cx.ctx, v, font(14.0, Weight::Medium), cx.pal.color1s, w * 0.5));
        let text_h = 19.5 + desc.as_ref().map_or(0.0, |d| d.size().y);
        let h = text_h.max(value.as_ref().map_or(28.0, |v| v.size().y)).max(28.0) + 12.0;
        heights.push(h);
        galleys.push((label, desc, value));
    }
    let total: f32 = heights.iter().sum();
    let table = Rect::from_min_size(pos2(x, cursor), vec2(w, total));
    let painter = cx.painter(ui, table);
    let mark = cx.fx.mark();
    cx.fx.fill(table, [10.0; 4], cx.pal.bg4);
    cx.fx.flush(&painter, mark);
    for (i, ((label, desc, value), h)) in galleys.into_iter().zip(&heights).enumerate() {
        let top = cursor;
        if i > 0 {
            painter.rect_filled(Rect::from_min_size(pos2(x, top), vec2(w, 1.0)), 0.0, cx.pal.bg5);
        }
        let text_h = 19.5 + desc.as_ref().map_or(0.0, |d| d.size().y);
        let ty = top + (h - text_h) / 2.0;
        painter.galley(pos2(x + 12.0, ty + (19.5 - label.size().y) / 2.0), label, cx.pal.color2);
        if let Some(d) = desc {
            painter.galley(pos2(x + 12.0, ty + 19.5), d, cx.pal.color3);
        }
        if let Some(v) = value {
            painter.galley(pos2(x + w - 12.0 - v.size().x, top + (h - v.size().y) / 2.0), v, cx.pal.color1s);
        } else {
            control(cx, ui, i, pos2(x + w - 12.0, top + h / 2.0));
        }
        cursor += h;
    }
    if let Some(n) = note {
        painter.text(pos2(x + 12.0, cursor + 6.0 + 12.0), Align2::LEFT_CENTER, n, font(11.5, Weight::Regular), cx.pal.color3);
        cursor += 6.0 + 24.0;
    }
    cursor - y
}

fn debug_section(app: &mut App, cx: &mut Cx, ui: &Ui, x: f32, mut y: f32, w: f32) -> f32 {
    let start = y;
    let rows = [Row { label: "Debug tools", description: "Frame stats overlay (F1), the live histogram switch (F2) and the colour space panel (F3).", value: None }];
    let mut toggled = false;
    let on = app.debug_tools;
    y += group(cx, ui, x, y, w, None, Some("Off in a release build until turned on here."), &rows, |cx, ui, _, right| {
        let resp = glass_toggle(cx, ui, Id::new("debug-tools"), pos2(right.x - 48.0, right.y - 14.0), 48.0, on, Stop { x: 3.0, w: 22.0 }, Stop { x: 23.0, w: 22.0 }, |_, _, _| {});
        toggled = resp.clicked();
        0.0
    });
    if toggled {
        app.set_debug_tools(!app.debug_tools);
    }
    y += 16.0;
    let info = &app.gpu.adapter_info;
    let rows = [
        Row { label: "Vendor", description: "", value: Some(format!("0x{:04x}", info.vendor)) },
        Row { label: "Architecture", description: "", value: Some(format!("{:?}", info.device_type)) },
        Row { label: "Device", description: "", value: Some(if info.name.is_empty() { "withheld".into() } else { info.name.clone() }) },
        Row { label: "Description", description: "", value: Some(format!("{} {}", info.driver, info.driver_info).trim().to_string()) },
        Row { label: "Fallback adapter", description: "", value: Some(if info.device_type == wgpu::DeviceType::Cpu { "yes" } else { "no" }.into()) },
    ];
    y += group(cx, ui, x, y, w, Some("Adapter"), None, &rows, |_, _, _, _| 0.0) + 16.0;
    let features = |f: wgpu::Features| {
        let list: Vec<String> = f.iter().map(|x| format!("{x:?}").to_lowercase().replace('_', "-")).collect();
        if list.is_empty() { "none".to_string() } else { list.join(", ") }
    };
    let rows = [
        Row { label: "Surface format", description: "", value: Some(format!("{:?}", app.gpu.target_format).to_lowercase()) },
        Row { label: "Working format", description: "", value: Some("rgba16float".into()) },
        Row { label: "Backend", description: "", value: Some(format!("{:?}", info.backend)) },
        Row { label: "Timestamp queries", description: "Per-pass GPU timings in the frame stats.", value: Some(if app.gpu.can_timestamp { "enabled" } else { "unavailable" }.into()) },
        Row { label: "Enabled features", description: "", value: Some(features(app.gpu.device.features())) },
        Row { label: "Adapter features", description: "", value: Some(features(app.gpu.adapter_features)) },
    ];
    y += group(cx, ui, x, y, w, Some("Device"), None, &rows, |_, _, _, _| 0.0) + 16.0;
    let limits = app.gpu.device.limits();
    let fmt = |v: u64| if v >= 1024 * 1024 { format!("{} MiB", v / (1024 * 1024)) } else { thousands(v) };
    let rows = [
        Row { label: "Max texture dimension 2D", description: "", value: Some(fmt(limits.max_texture_dimension_2d as u64)) },
        Row { label: "Max buffer size", description: "", value: Some(fmt(limits.max_buffer_size)) },
        Row { label: "Max storage buffer binding size", description: "", value: Some(fmt(limits.max_storage_buffer_binding_size as u64)) },
        Row { label: "Max uniform buffer binding size", description: "", value: Some(fmt(limits.max_uniform_buffer_binding_size as u64)) },
        Row { label: "Max compute invocations per workgroup", description: "", value: Some(fmt(limits.max_compute_invocations_per_workgroup as u64)) },
        Row { label: "Max compute workgroup storage size", description: "", value: Some(fmt(limits.max_compute_workgroup_storage_size as u64)) },
        Row { label: "Max sampled textures per shader stage", description: "", value: Some(fmt(limits.max_sampled_textures_per_shader_stage as u64)) },
        Row { label: "Max storage textures per shader stage", description: "", value: Some(fmt(limits.max_storage_textures_per_shader_stage as u64)) },
    ];
    y += group(cx, ui, x, y, w, Some("Limits"), None, &rows, |_, _, _, _| 0.0);
    y - start
}

fn thousands(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const PLUS: &str = "M12 8v8M8 12h8";
const SMALL_REVERT_ARC: &str = "M12.6 8.4A4 4 0 1 1 8 12.9";
const SMALL_REVERT_HEAD: &str = "M9.5 8.4 12 6.5 12 10.3Z";

fn shortcuts_section(app: &mut App, cx: &mut Cx, ui: &Ui, x: f32, mut y: f32, w: f32) -> f32 {
    let start = y;
    for g in GROUPS.iter() {
        let painter = cx.painter(ui, Rect::from_min_size(pos2(x, y), vec2(w, 24.0)));
        painter.text(pos2(x + 12.0, y + 12.0), Align2::LEFT_CENTER, g.label, font(15.0, Weight::SemiBold), cx.pal.color1);
        y += 28.0;
        let actions: Vec<_> = actions_in(g.id).collect();
        // Rows lay out their chips first, to know how tall they are.
        let mut layouts = Vec::new();
        for a in &actions {
            let bindings = app.shortcuts.bindings(a.id);
            let notice = app.shortcut_notice.as_ref().filter(|(id, _)| *id == a.id).map(|(_, n)| n.clone()).filter(|n| !n.is_empty());
            let desc_h = if a.description.is_empty() { 0.0 } else { wrap(&cx.ctx, a.description, font(11.5, Weight::Regular), cx.pal.color3, w * 0.45).size().y };
            let notice_h = if notice.is_some() { 15.5 } else { 0.0 };
            let text_h = 19.5 + desc_h + notice_h;
            // The chips inherit the page's 24px line, so a row is never under 26.
            let border = if layouts.is_empty() { 0.0 } else { 1.0 };
            layouts.push((bindings, notice, text_h.max(26.0) + 16.0 + border));
        }
        let total: f32 = layouts.iter().map(|l| l.2).sum();
        let table = Rect::from_min_size(pos2(x, y), vec2(w, total));
        let painter = cx.painter(ui, table);
        let mark = cx.fx.mark();
        cx.fx.fill(table, [10.0; 4], cx.pal.bg4);
        cx.fx.flush(&painter, mark);
        for (i, (a, (bindings, notice, h))) in actions.iter().zip(layouts).enumerate() {
            let top = y;
            if i > 0 {
                painter.rect_filled(Rect::from_min_size(pos2(x, top), vec2(w, 1.0)), 0.0, cx.pal.bg5);
            }
            let top = top + if i > 0 { 1.0 } else { 0.0 };
            let mut ty = top + 8.0;
            painter.text(pos2(x + 12.0, ty + 9.75), Align2::LEFT_CENTER, a.label, font(13.0, Weight::Regular), cx.pal.color2);
            ty += 19.5;
            if !a.description.is_empty() {
                let g = wrap(&cx.ctx, a.description, font(11.5, Weight::Regular), cx.pal.color3, w * 0.45);
                let gh = g.size().y;
                painter.galley(pos2(x + 12.0, ty), g, cx.pal.color3);
                ty += gh;
            }
            if let Some(n) = &notice {
                painter.text(pos2(x + 12.0, ty + 7.75), Align2::LEFT_CENTER, n, font(11.5, Weight::Regular), cx.pal.color1s);
            }

            // Keys, right to left: the revert, the add button, then the chips.
            let mut right = x + w - 12.0;
            let cy = top + 8.0 + 13.0;
            let custom = app.shortcuts.is_custom(a.id);
            let shown = cx.anim.flag(Id::new(("sc-revert", a.key)), custom, 0.25, EASE);
            let saved = (cx.opacity, cx.enabled);
            cx.opacity *= shown;
            cx.enabled &= custom;
            let ink = cx.pal.color2;
            let r = Rect::from_min_size(pos2(right - 24.0, cy - 12.0), vec2(24.0, 24.0));
            let resp = glass_button(cx, ui, Id::new(("sc-reset", a.key)), r, false, false, |p, r, s| {
                let placed = Placed::new(r.center() - vec2(12.0, 12.0) * s, s);
                placed.stroke(p, SMALL_REVERT_ARC, Stroke::new(1.2_f32 * s, ink));
                placed.fill(p, SMALL_REVERT_HEAD, ink);
            });
            if resp.clicked() && custom {
                app.shortcuts.reset(a.id);
            }
            (cx.opacity, cx.enabled) = saved;
            right -= 24.0 + 6.0;
            let r = Rect::from_min_size(pos2(right - 24.0, cy - 12.0), vec2(24.0, 24.0));
            let resp = glass_button(cx, ui, Id::new(("sc-add", a.key)), r, false, false, |p, r, s| {
                Placed::new(r.center() - vec2(12.0, 12.0) * s, s).stroke(p, PLUS, Stroke::new(1.4_f32 * s, ink));
            });
            if resp.clicked() {
                app.shortcuts.recording = Some(a.id);
                app.shortcut_notice = None;
            }
            right -= 24.0 + 6.0;
            let recording = app.shortcuts.recording == Some(a.id);
            if recording {
                let label = "Press keys…";
                let cw = text_width(&cx.ctx, label, font(12.0, Weight::Medium)) + 16.0;
                let chip_rect = Rect::from_min_size(pos2(right - cw, cy - 11.0), vec2(cw, 22.0));
                let pulse = 0.725 + 0.275 * ((cx.now() * std::f64::consts::TAU / 1.1).cos() as f32);
                let mark = cx.fx.mark();
                cx.fx.fill(chip_rect, [6.0; 4], fade(cx.pal.bg5, pulse));
                cx.fx.ring(chip_rect, [6.0; 4], 1.0, fade(cx.pal.color1s, 0.45 * pulse));
                cx.fx.flush(&painter, mark);
                painter.text(chip_rect.center(), Align2::CENTER_CENTER, label, font(12.0, Weight::Medium), fade(cx.pal.color1s, pulse));
                right -= cw + 6.0;
                cx.ctx.request_repaint();
            } else if bindings.is_empty() {
                painter.text(pos2(right, cy), Align2::RIGHT_CENTER, "Unbound", egui::FontId::new(12.0, egui::FontFamily::Name("fig400".into())), cx.pal.color4);
            }
            for b in bindings.iter().rev() {
                let label = display(b);
                let cw = text_width(&cx.ctx, &label, font(12.0, Weight::Medium)) + 8.0 + 4.0 + 14.0 + 4.0;
                let chip_rect = Rect::from_min_size(pos2(right - cw, cy - 11.0), vec2(cw, 22.0));
                let mark = cx.fx.mark();
                cx.fx.fill(chip_rect, [6.0; 4], cx.pal.bg5);
                cx.fx.ring(chip_rect, [6.0; 4], 1.0, white(0.08));
                cx.fx.flush(&painter, mark);
                painter.text(pos2(chip_rect.left() + 8.0, cy), Align2::LEFT_CENTER, &label, font(12.0, Weight::Medium), cx.pal.color1s);
                let clear = Rect::from_min_size(pos2(chip_rect.right() - 4.0 - 14.0, cy - 7.0), vec2(14.0, 14.0));
                let resp = ui.interact(clear, Id::new(("sc-clear", a.key, b.as_str())), cx.sense(Sense::click()));
                let lit = cx.anim.flag(Id::new(("sc-clear-lit", a.key, b.as_str())), resp.hovered(), 0.12, EASE);
                if lit > 0.0 {
                    let mark = cx.fx.mark();
                    cx.fx.fill(clear, [4.0; 4], white(0.1 * lit));
                    cx.fx.flush(&painter, mark);
                }
                let ink = mix(cx.pal.color3, cx.pal.color1, lit);
                let c = clear.center();
                painter.line_segment([c - vec2(3.0, 3.0), c + vec2(3.0, 3.0)], Stroke::new(1.4_f32, ink));
                painter.line_segment([c + vec2(3.0, -3.0), c + vec2(-3.0, 3.0)], Stroke::new(1.4_f32, ink));
                if resp.clicked() {
                    app.shortcuts.remove(a.id, b);
                }
                right -= cw + 6.0;
            }
            y += h;
        }
        if !g.note.is_empty() {
            painter.text(pos2(x + 12.0, y + 6.0 + 12.0), Align2::LEFT_CENTER, g.note, font(11.5, Weight::Regular), cx.pal.color3);
            y += 6.0 + 24.0;
        }
        y += 16.0;
    }
    // The foot: what the store holds, and the way back to the defaults.
    let any = app.shortcuts.is_any_custom();
    let painter = cx.painter(ui, Rect::from_min_size(pos2(x, y), vec2(w, 32.0)));
    painter.text(
        pos2(x + 12.0, y + 16.0),
        Align2::LEFT_CENTER,
        if any { "Some shortcuts have been changed." } else { "Every shortcut is at its default." },
        font(11.5, Weight::Regular),
        cx.pal.color3,
    );
    let label = "Restore defaults";
    let bw = text_width(&cx.ctx, label, font(12.5, Weight::Medium)) + 24.0;
    let r = Rect::from_min_size(pos2(x + w - bw, y), vec2(bw, 32.0));
    let resp = ui.interact(r, Id::new("sc-restore"), if any { cx.sense(Sense::click()) } else { Sense::hover() });
    let lit = cx.anim.flag("sc-restore-lit", resp.hovered() && any, 0.12, EASE);
    let mark = cx.fx.mark();
    cx.fx.fill(r, [8.0; 4], mix(cx.pal.bg4, cx.pal.bg5, lit));
    cx.fx.flush(&painter, mark);
    let ink = if any { mix(cx.pal.color2, cx.pal.color1, lit) } else { cx.pal.color4 };
    painter.text(r.center(), Align2::CENTER_CENTER, label, font(12.5, Weight::Medium), ink);
    if resp.clicked() && any {
        app.shortcuts.reset_all();
    }
    y += 32.0;
    y - start
}

pub fn settings_modal(app: &mut App, cx: &mut Cx, ui: &mut Ui) {
    let open = app.settings_open;
    let t = cx.anim.flag("settings-scrim", open, 0.15, EASE);
    if !open {
        cx.anim.set("settings-scrim", 0.0);
        cx.anim.set("settings-dialog", 0.0);
        return;
    }
    let screen = cx.ctx.screen_rect();
    let area = Rect::from_min_max(pos2(12.0, 44.0), pos2(screen.right() - 12.0, screen.bottom() - 12.0));
    let size = vec2(1100f32.min(screen.width() * 0.92), 800f32.min(screen.height() * 0.9));
    let dialog = Rect::from_center_size(area.center(), size);

    // The scrim: one dim under the dialog, and a click on it closes.
    let scrim = ui.interact(screen, Id::new("settings-scrim"), Sense::click());
    let painter = cx.painter(ui, screen);
    let mark = cx.fx.mark();
    cx.fx.fill(screen, [0.0; 4], black(0.6 * t));
    cx.fx.flush(&painter, mark);

    let d = cx.anim.flag("settings-dialog", true, 0.18, EASE_OUT_EMPH);
    let shown = Rect::from_center_size(dialog.center() + vec2(0.0, -6.0 * (1.0 - d)), dialog.size() * (0.985 + 0.015 * d));
    let s = shown.width() / dialog.width();
    let lap = 14.0;
    let sidebar = Rect::from_min_size(shown.min, vec2((190.0 + lap) * s, shown.height()));
    let panel = Rect::from_min_max(pos2(sidebar.right() - lap * s, shown.top()), shown.max);
    let mark = cx.fx.mark();
    cx.fx.shadow(shown, 14.0, &shadow(0.0, 20.0, 60.0, 0.0, black(0.65)));
    let mut glass = GlassStyle::panel(black(0.78));
    glass.rim = [0.0; 4];
    // The scrim dims what the sidebar frosts.
    glass.backdrop_level = 0.4;
    cx.fx.glass(sidebar, [14.0, 0.0, 0.0, 14.0], &glass);
    cx.fx.fill(panel, [14.0; 4], cx.pal.bg3);
    cx.fx.flush(&painter, mark);
    super::sidebar::block(ui, Id::new("settings-dialog-block"), shown);
    if scrim.clicked() && !shown.contains(scrim.interact_pointer_pos().unwrap_or(Pos2::ZERO)) {
        app.settings_open = false;
    }

    // Categories.
    painter.text(pos2(sidebar.left() + 16.0, sidebar.top() + 8.0 + 6.0 + 12.0), Align2::LEFT_CENTER, "Settings", font(18.0, Weight::Medium), cx.pal.color1);
    let mut y = sidebar.top() + 8.0 + 6.0 + 24.0 + 10.0;
    for (i, (label, icon)) in SECTIONS.iter().enumerate() {
        let r = Rect::from_min_size(pos2(sidebar.left() + 8.0, y), vec2(190.0 - 16.0 + 8.0, 32.0));
        let id = Id::new(("settings-nav", i));
        let resp = ui.interact(r, id, Sense::click());
        let active = app.settings_section == i;
        let hover = cx.anim.flag(id.with("hover"), resp.hovered() && !active, 0.12, EASE);
        let mark = cx.fx.mark();
        if active {
            let mut chip_style = GlassStyle::chip();
            chip_style.tint = white(0.08);
            chip_style.rim = [0.2, 0.09, 0.05, 0.1];
            chip_style.gloss = [0.0, 0.0];
            cx.fx.glass(r, [8.0; 4], &chip_style);
        } else if hover > 0.0 {
            cx.fx.fill(r, [8.0; 4], white(0.06 * hover));
        }
        cx.fx.flush(&painter, mark);
        let ink = if active { cx.pal.color1 } else { mix(cx.pal.color2, cx.pal.color1, hover) };
        Placed::new(pos2(r.left() + 8.0, r.center().y - 8.0), 16.0 / 24.0).stroke(&painter, icon, Stroke::new(2.0_f32 * 16.0 / 24.0, ink));
        painter.text(pos2(r.left() + 8.0 + 16.0 + 10.0, r.center().y), Align2::LEFT_CENTER, *label, font(13.0, Weight::Regular), ink);
        if resp.clicked() {
            app.settings_section = i;
        }
        y += 34.0;
    }

    // The active panel: heading bar, then the scrolling body under it.
    let body = Rect::from_min_max(pos2(panel.left(), panel.top() + 56.0), panel.max);
    let content_x = panel.left() + 20.0;
    let content_w = panel.width() - 20.0 - 6.0 - 14.0;
    let scroll_id = Id::new(("settings-scroll", app.settings_section));
    let mut scroll: f32 = cx.ctx.data(|d| d.get_temp(scroll_id)).unwrap_or(0.0);
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    child.set_clip_rect(body.intersect(panel));
    let top = body.top() + 36.0 - 56.0 + 56.0 - scroll;
    let height = match app.settings_section {
        0 => shortcuts_section(app, cx, &child, content_x, top, content_w),
        _ => debug_section(app, cx, &child, content_x, top, content_w),
    } + 36.0 + 20.0;
    let viewport = body.height();
    let max_scroll = (height - viewport).max(0.0);
    if child.rect_contains_pointer(body) {
        scroll -= cx.ctx.input(|i| i.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, max_scroll);
    cx.ctx.data_mut(|d| d.insert_temp(scroll_id, scroll));
    if max_scroll > 0.0 {
        let track = Rect::from_min_max(pos2(panel.right() - 14.0, body.top()), panel.max);
        let thumb_h = (viewport / height * track.height()).max(24.0);
        let thumb = Rect::from_min_size(pos2(track.left() + 5.5, track.top() + 5.5 + scroll / max_scroll * (track.height() - thumb_h - 11.0)), vec2(3.0, thumb_h));
        let hover = cx.anim.flag("settings-thumb", child.rect_contains_pointer(track), 0.12, EASE);
        let mark = cx.fx.mark();
        cx.fx.fill(thumb, [1.5; 4], mix(cx.pal.slider_track, cx.pal.color3, hover));
        cx.fx.flush(&child.painter().clone(), mark);
    }

    // The heading's cover, its fade, and the title over them.
    let head = Rect::from_min_size(panel.min, vec2(panel.width(), 56.0));
    let p = ui.painter().clone().with_clip_rect(panel);
    let mark = cx.fx.mark();
    let darker = super::theme::mix(cx.pal.bg3, Color32::BLACK, 0.12);
    cx.fx.linear(head, [14.0, 14.0, 0.0, 0.0], head.left_top(), head.left_bottom(), darker, cx.pal.bg3);
    let fade_rect = Rect::from_min_size(head.left_bottom(), vec2(panel.width() - 14.0, 32.0));
    cx.fx.linear(fade_rect, [0.0; 4], fade_rect.left_top(), fade_rect.left_bottom(), cx.pal.bg3, fade(cx.pal.bg3, 0.0));
    cx.fx.flush(&p, mark);
    p.text(pos2(panel.left() + 20.0, head.center().y), Align2::LEFT_CENTER, SECTIONS[app.settings_section].0, outfit(25.0), cx.pal.color1);
    let ink = cx.pal.color2;
    let close = Rect::from_min_size(pos2(panel.right() - 10.0 - 28.0, panel.top() + 10.0), vec2(28.0, 28.0));
    let resp = glass_button(cx, ui, Id::new("settings-close"), close, false, false, |p, r, s| {
        let placed = Placed::new(r.center() - vec2(14.0, 14.0) * s, 28.0 / 24.0 * s);
        placed.stroke(p, "M8.6 8.6l6.8 6.8M15.4 8.6l-6.8 6.8", Stroke::new(1.4_f32 * 28.0 / 24.0 * s, ink));
    });
    if resp.clicked() {
        app.settings_open = false;
    }
    if resp.hovered() {
        cx.ctx.set_cursor_icon(CursorIcon::Default);
    }
    }
