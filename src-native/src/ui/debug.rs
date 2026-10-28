// src-native/src/ui/debug.rs
//
// The diagnostics: the frame stats overlay (F1) and the colour space panel
// (F3) with its preview card.

use egui::{pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui, UiBuilder};

use super::cx::{text_width, Cx};
use super::sidebar::SMALL_SHADOWS;
use super::theme::{black, mono, rgba, white};
use crate::app::{App, UpdateSample};
use crate::export_rendering::RolloffMode;
use crate::gpu::chrome::{shadow, GlassStyle};
use crate::gpu::color_space::GAMUT_TINTS;
use crate::gpu::frame_timer::PassTiming;
use crate::model::viewport::{valid_box, BoxF, ChainRegion};
use crate::output_space::OutputSpace;

const WINDOW: usize = 30;
const GRAPH: egui::Vec2 = egui::vec2(168.0, 34.0);
const BUDGET_MS: f32 = 1000.0 / 60.0;
const IDLE_AFTER: f64 = 0.5;
const SIZE: f32 = 10.5;

/// Each pass averaged over the frames that carried it, in first-seen order.
fn mean_passes(all: &[PassTiming]) -> Vec<PassTiming> {
    let mut out: Vec<(String, f64, u32)> = Vec::new();
    for p in all {
        match out.iter_mut().find(|e| e.0 == p.label) {
            Some(e) => {
                e.1 += p.ms;
                e.2 += 1;
            }
            None => out.push((p.label.clone(), p.ms, 1)),
        }
    }
    out.into_iter().map(|(label, total, n)| PassTiming { label, ms: total / n as f64 }).collect()
}

fn ms(v: f64) -> String {
    format!("{v:.1} ms")
}

struct Lines {
    rows: Vec<(String, String, Color32, Color32, f32)>,
}

impl Lines {
    fn row(&mut self, k: impl Into<String>, v: impl Into<String>, kc: Color32, vc: Color32, indent: f32) {
        self.rows.push((k.into(), v.into(), kc, vc, indent));
    }
}

fn mean_update(samples: &[UpdateSample]) -> Option<(f64, f64, Vec<PassTiming>)> {
    let recent = &samples[samples.len().saturating_sub(WINDOW)..];
    if recent.is_empty() {
        return None;
    }
    let record = recent.iter().map(|s| s.record_ms).sum::<f64>() / recent.len() as f64;
    let passes = mean_passes(&recent.iter().filter(|s| !s.passes.is_empty()).flat_map(|s| s.passes.clone()).collect::<Vec<_>>());
    let gpu = passes.iter().map(|p| p.ms).sum();
    Some((gpu, record, passes))
}

pub fn stats_overlay(app: &mut App, cx: &mut Cx, ui: &Ui, slot: Rect) {
    let samples = app.debug.frame_samples.lock().unwrap().clone();
    let recent = &samples[samples.len().saturating_sub(WINDOW)..];
    let idle = app.last_frame_at.is_none_or(|t| app.now() - t > IDLE_AFTER);

    let dim = rgba(255, 255, 255, 0.55);
    let total_c = rgba(255, 255, 255, 0.82);
    let child_c = rgba(255, 255, 255, 0.45);
    let warn = Color32::from_rgb(240, 150, 90);
    let good = Color32::from_rgb(120, 200, 150);

    let mut sections: Vec<Lines> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut headline = ("idle".to_string(), "—".to_string());
    if !recent.is_empty() {
        let n = recent.len() as f64;
        let intervals: Vec<f64> = recent.iter().map(|s| s.interval_ms).filter(|&v| v > 0.0).collect();
        let interval = if intervals.is_empty() { 0.0 } else { intervals.iter().sum::<f64>() / intervals.len() as f64 };
        let acquire = recent.iter().map(|s| s.acquire_ms).sum::<f64>() / n;
        let record = recent.iter().map(|s| s.record_ms).sum::<f64>() / n;
        let turnaround = recent.iter().map(|s| s.turnaround_ms).sum::<f64>() / n;
        let fps = if interval > 0.0 { 1000.0 / interval } else { 0.0 };
        headline = (if idle { "idle".into() } else { format!("{fps:.0} fps") }, ms(interval));
        let mut l = Lines { rows: Vec::new() };
        l.row("acquire", ms(acquire), dim, if acquire > 2.0 { warn } else { total_c }, 0.0);
        l.row("record", ms(record), dim, total_c, 0.0);
        l.row("turnaround", ms(turnaround), dim, total_c, 0.0);
        sections.push(l);
        let passes = mean_passes(&recent.iter().filter(|s| !s.passes.is_empty()).flat_map(|s| s.passes.clone()).collect::<Vec<_>>());
        if !passes.is_empty() {
            let mut l = Lines { rows: Vec::new() };
            l.row("gpu passes", ms(passes.iter().map(|p| p.ms).sum()), total_c, total_c, 0.0);
            for p in passes {
                l.row(p.label, ms(p.ms), child_c, rgba(255, 255, 255, 0.62), 9.6);
            }
            sections.push(l);
        } else if !app.gpu.can_timestamp {
            notes.push("no timestamp-query on this adapter".into());
        }
        if !app.debug.histogram {
            notes.push("histogram off (F2)".into());
        } else if let Some((gpu, rec, passes)) = mean_update(&app.debug.histogram_samples) {
            let mut l = Lines { rows: Vec::new() };
            l.row("histogram gpu", ms(gpu), total_c, total_c, 0.0);
            for p in passes {
                l.row(p.label, ms(p.ms), child_c, rgba(255, 255, 255, 0.62), 9.6);
            }
            l.row("histogram cpu", ms(rec), total_c, total_c, 0.0);
            sections.push(l);
        }
        if app.colour_space_debug.enabled {
            if let Some((gpu, rec, passes)) = mean_update(&app.debug.colour_space_samples) {
                let mut l = Lines { rows: Vec::new() };
                l.row("colour space gpu", ms(gpu), total_c, total_c, 0.0);
                for p in passes {
                    l.row(p.label, ms(p.ms), child_c, rgba(255, 255, 255, 0.62), 9.6);
                }
                l.row("colour space cpu", ms(rec), total_c, total_c, 0.0);
                sections.push(l);
            }
        }
    } else {
        notes.push("move a slider to sample".into());
    }

    let line = 15.2;
    let mut height = 8.0 + 17.0 + 5.6 + GRAPH.y;
    for (i, s) in sections.iter().enumerate() {
        height += if i == 0 { 6.4 } else { 4.8 + 4.8 + 1.0 } + s.rows.len() as f32 * line;
    }
    height += notes.len() as f32 * (6.4 + line) + 8.0;
    let rect = Rect::from_min_size(pos2(slot.left() + 8.0, slot.top() + 8.0), vec2(GRAPH.x + 19.2 + 2.0, height));
    let painter = cx.painter(ui, rect);
    let mark = cx.fx.mark();
    let mut style = GlassStyle::panel(rgba(18, 18, 20, 0.72));
    style.rim = [0.0; 4];
    cx.fx.glass(rect, [8.0; 4], &style);
    cx.fx.ring(rect, [8.0; 4], 1.0, white(0.1));
    let mut y = rect.top() + 8.0;
    let x0 = rect.left() + 9.6 + 1.0;
    let x1 = rect.right() - 9.6 - 1.0;
    let graph = Rect::from_min_size(pos2(x0, y + 17.0 + 5.6), GRAPH);
    cx.fx.fill(graph, [3.0; 4], white(0.04));
    // Scaled to the worst frame on screen, never tighter than the budget.
    let peak = samples.iter().map(|s| s.turnaround_ms as f32).fold(BUDGET_MS, f32::max);
    let bar_w = GRAPH.x / samples.len().max(1) as f32;
    let gy = |v: f32| graph.bottom() - v / peak * GRAPH.y;
    for (i, s) in samples.iter().enumerate() {
        let t = s.turnaround_ms as f32;
        let c = if t > BUDGET_MS { warn } else { good };
        cx.fx.fill(Rect::from_min_max(pos2(graph.left() + i as f32 * bar_w, gy(t)), pos2(graph.left() + i as f32 * bar_w + (bar_w - 0.5).max(0.5), graph.bottom())), [0.0; 4], c);
    }
    cx.fx.flush(&painter, mark);
    let dash = egui::Shape::dashed_line(&[pos2(graph.left(), gy(BUDGET_MS)), pos2(graph.right(), gy(BUDGET_MS))], Stroke::new(1.0, white(0.22)), 2.0, 3.0);
    painter.extend(dash);

    let fps_colour = if headline.0 == "idle" { white(0.4) } else { good };
    painter.text(pos2(x0, y + 8.5), Align2::LEFT_CENTER, &headline.0, egui::FontId::monospace(12.0), fps_colour);
    painter.text(pos2(x1, y + 8.5), Align2::RIGHT_CENTER, &headline.1, egui::FontId::monospace(12.0), white(0.6));
    y = graph.bottom();
    for (i, s) in sections.iter().enumerate() {
        if i == 0 {
            y += 6.4;
        } else {
            y += 4.8;
            painter.rect_filled(Rect::from_min_size(pos2(x0, y), vec2(x1 - x0, 1.0)), 0.0, white(0.1));
            y += 1.0 + 4.8;
        }
        for (k, v, kc, vc, indent) in &s.rows {
            painter.text(pos2(x0 + indent, y + line / 2.0), Align2::LEFT_CENTER, k, mono(SIZE), *kc);
            painter.text(pos2(x1, y + line / 2.0), Align2::RIGHT_CENTER, v, mono(SIZE), *vc);
            y += line;
        }
    }
    for n in notes {
        y += 6.4;
        painter.text(pos2(x0, y + line / 2.0), Align2::LEFT_CENTER, n, mono(SIZE), white(0.45));
        y += line;
    }
}

// ── The colour space panel ────────────────────────────────────────────────────

fn heading(painter: &egui::Painter, at: Pos2, text: &str) {
    let mut x = at.x;
    for ch in text.to_uppercase().chars() {
        let s = ch.to_string();
        let r = painter.text(pos2(x, at.y), Align2::LEFT_TOP, &s, egui::FontId::new(SIZE, egui::FontFamily::Name("fig600".into())), white(0.5));
        x = r.right() + SIZE * 0.04;
    }
}

/// The browser's little checkbox and radio, at 11px.
fn tick(cx: &mut Cx, ui: &Ui, id: Id, at: Pos2, checked: bool, round: bool) -> egui::Response {
    let r = Rect::from_min_size(at, vec2(11.0, 11.0));
    let resp = ui.interact(r, id, cx.sense(Sense::click()));
    let painter = ui.painter();
    let accent = rgba(255, 255, 255, 0.72);
    let radius = if round { 5.5 } else { 2.0 };
    if checked {
        painter.rect_filled(r, radius, accent);
        if round {
            painter.circle_filled(r.center(), 2.5, Color32::BLACK);
        } else {
            painter.add(egui::Shape::line(vec![r.min + vec2(2.5, 5.5), r.min + vec2(4.6, 7.8), r.min + vec2(8.6, 3.2)], Stroke::new(1.5, Color32::BLACK)));
        }
    } else {
        painter.rect_stroke(r, radius, Stroke::new(1.0, white(0.5)), egui::StrokeKind::Inside);
    }
    resp
}

fn note(painter: &egui::Painter, ctx: &egui::Context, x: f32, y: f32, w: f32, text: &str) -> f32 {
    painter.rect_filled(Rect::from_min_size(pos2(x, y + 8.0), vec2(w, 1.0)), 0.0, white(0.1));
    let g = ctx.fonts(|f| f.layout(text.to_string(), mono(SIZE), white(0.38), w));
    let h = g.size().y;
    painter.galley(pos2(x, y + 8.0 + 6.4), g, white(0.38));
    8.0 + 6.4 + h
}

fn panel_glass(cx: &mut Cx, painter: &egui::Painter, r: Rect) {
    let mark = cx.fx.mark();
    for s in &SMALL_SHADOWS {
        cx.fx.shadow(r, 12.0, s);
    }
    cx.fx.glass(r, [12.0; 4], &GlassStyle::panel(cx.pal.glass_tint));
    cx.fx.inset(r, 12.0, &shadow(16.0, 21.0, 50.0, -38.0, white(0.16)));
    cx.fx.flush(painter, mark);
}

fn format_percentage(v: f64) -> String {
    if v == 0.0 {
        "0%".into()
    } else if v < 0.1 {
        "<0.1%".into()
    } else {
        format!("{v:.1}%")
    }
}

/// One glass card of the column, laid out by `body` from the top inside its
/// padding, which returns the height it used.
fn card(cx: &mut Cx, ui: &mut Ui, x: f32, y: f32, w: f32, key: &str, heights: &mut std::collections::HashMap<String, f32>, body: impl FnOnce(&mut Cx, &mut Ui, f32, f32, f32) -> f32) -> f32 {
    let h = heights.get(key).copied().unwrap_or(100.0);
    let r = Rect::from_min_size(pos2(x, y), vec2(w, h));
    let painter = cx.painter(ui, r);
    panel_glass(cx, &painter, r);
    super::sidebar::block(ui, Id::new(("cs-card", key)), r);
    let used = body(cx, ui, x + 10.0, y + 10.0, w - 20.0) + 20.0;
    heights.insert(key.to_string(), used);
    used
}

pub fn colour_space_panel(app: &mut App, cx: &mut Cx, ui: &mut Ui, bar: Rect) {
    let mut heights = std::mem::take(&mut app.card_heights);
    let mut child = ui.new_child(UiBuilder::new().max_rect(bar));
    child.set_clip_rect(bar);
    let (x, w) = (bar.left(), bar.width() - 28.0);
    let scroll = app.colour_bar_scroll;
    let mut y = bar.top() - scroll;
    let start = y;
    let d = &mut app.colour_space_debug;
    let mut overlays_changed = false;

    y += card(cx, &mut child, x, y, w, "rolloff", &mut heights, |cx, ui, x, y, w| {
        let painter = ui.painter().clone();
        heading(&painter, pos2(x, y), "Highlight roll-off");
        let mut cy = y + 15.0 + 5.6;
        // Strength, as a range input.
        painter.text(pos2(x, cy + 7.6), Align2::LEFT_CENTER, "Strength", mono(SIZE), white(0.78));
        let value = format!("{}", d.rolloff.round());
        let label_w = text_width(&cx.ctx, "Strength", mono(SIZE)) + 7.2;
        let track = Rect::from_min_max(pos2(x + label_w, cy + 2.0), pos2(x + w - 24.0 - 7.2, cy + 13.0));
        let resp = ui.interact(track, Id::new("rolloff-range"), cx.sense(Sense::click_and_drag()));
        if let Some(p) = resp.interact_pointer_pos() {
            if resp.dragged() || resp.clicked() {
                d.rolloff = (((p.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0).round();
            }
        }
        if resp.double_clicked() {
            d.rolloff = 0.0;
        }
        let f = d.rolloff / 100.0;
        let mid = track.center().y;
        painter.rect_filled(Rect::from_min_max(pos2(track.left(), mid - 2.0), pos2(track.right(), mid + 2.0)), 2.0, white(0.2));
        painter.rect_filled(Rect::from_min_max(pos2(track.left(), mid - 2.0), pos2(track.left() + track.width() * f, mid + 2.0)), 2.0, rgba(255, 255, 255, 0.72));
        painter.circle_filled(pos2(track.left() + track.width() * f, mid), 5.5, rgba(255, 255, 255, 0.9));
        painter.text(pos2(x + w, cy + 7.6), Align2::RIGHT_CENTER, value, mono(SIZE), white(0.45));
        cy += 15.2 + 3.8;
        for (mode, label) in [(RolloffMode::Luminance, "Luminance"), (RolloffMode::PerChannel, "Per channel")] {
            let resp = tick(cx, ui, Id::new(("rolloff-mode", label)), pos2(x, cy + 2.0), d.rolloff_mode == mode, true);
            painter.text(pos2(x + 11.0 + 7.2, cy + 7.6), Align2::LEFT_CENTER, label, mono(SIZE), white(0.78));
            if resp.clicked() {
                d.rolloff_mode = mode;
            }
            cy += 15.2 + 3.8;
        }
        let text = match d.rolloff_mode {
            RolloffMode::Luminance => "Compresses brightness and keeps each hue, which fades to white as it nears the top. ",
            RolloffMode::PerChannel => "Compresses R, G and B separately, like film: bright colours desaturate sooner but their hues can drift. ",
        };
        cy += note(&painter, &cx.ctx, x, cy - 3.8, w, &format!("{text}Applied to the preview, histogram and export, and kept when this panel is closed."));
        cy - y
    }) + 12.0;

    let chroma_size = &mut app.chroma_size;
    let stats = d.stats.clone();
    let chroma_id = app.chroma_plot.id;
    let has_image = app.image.is_some();
    let proof = app.proofing.then_some(app.export.settings.output_space);
    y += card(cx, &mut child, x, y, w, "chroma", &mut heights, |cx, ui, x, y, w| {
        let painter = ui.painter().clone();
        heading(&painter, pos2(x, y), "Chromaticity · CIE 1931");
        let mut cy = y + 15.0 + 5.6;
        let plot = Rect::from_min_size(pos2(x, cy), vec2(w, w * 9.0 / 8.0));
        *chroma_size = (plot.width() * cx.ppp, plot.height() * cx.ppp);
        painter.rect_filled(plot, 6.0, Color32::BLACK);
        if let (Some(id), true) = (chroma_id, has_image) {
            painter.add(egui::epaint::RectShape::filled(plot, 6.0, Color32::WHITE).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
        }
        cy = plot.bottom() + 4.8;
        painter.text(pos2(x, cy), Align2::LEFT_TOP, "x", mono(SIZE), white(0.4));
        painter.text(pos2(x + w, cy), Align2::RIGHT_TOP, "y", mono(SIZE), white(0.4));
        cy += 15.2 + 8.8;
        painter.rect_filled(Rect::from_min_size(pos2(x, cy - 7.2), vec2(w, 1.0)), 0.0, white(0.1));
        for (slot, space) in OutputSpace::ALL.iter().enumerate() {
            let resp = tick(cx, ui, Id::new(("gamut", slot)), pos2(x, cy + 2.0), d.overlays[slot], false);
            if resp.clicked() {
                d.overlays[slot] = !d.overlays[slot];
                overlays_changed = true;
            }
            let tint = super::theme::hex(GAMUT_TINTS[slot]);
            painter.rect_filled(Rect::from_min_size(pos2(x + 18.2, cy + 3.0), vec2(9.0, 9.0)), 2.0, tint);
            painter.text(pos2(x + 34.4, cy + 7.6), Align2::LEFT_CENTER, super::overlays::SPACE_LABELS[slot], mono(SIZE), white(0.78));
            let outside = if *space == OutputSpace::Rec2020 && !d.transformed {
                "working".to_string()
            } else if let Some(s) = stats.as_ref().filter(|s| d.overlays[slot] && s.counted > 0) {
                format!("{} out", format_percentage(100.0 * s.outside[slot] as f64 / s.counted as f64))
            } else if d.overlays[slot] {
                "…".into()
            } else {
                String::new()
            };
            painter.text(pos2(x + w, cy + 7.6), Align2::RIGHT_CENTER, outside, mono(SIZE), white(0.45));
            cy += 15.2 + 3.8;
        }
        cy += 5.6;
        let resp = tick(cx, ui, Id::new("transformed"), pos2(x, cy + 2.0), d.transformed, false);
        painter.text(pos2(x + 18.2, cy + 7.6), Align2::LEFT_CENTER, "After output transform", mono(SIZE), white(0.78));
        if resp.clicked() {
            d.transformed = !d.transformed;
            overlays_changed = true;
        }
        cy += 15.2 + 3.8;
        let text = if d.transformed {
            format!(
                "What the preview is showing, fitted to sRGB{}. Nothing can fall outside the space being rendered to, so a zero here says the compression worked.",
                proof.map(|p| format!(" and proofed against {}", super::overlays::SPACE_LABELS[OutputSpace::ALL.iter().position(|s| *s == p).unwrap_or(0)])).unwrap_or_default()
            )
        } else {
            "The colour the adjustments are asking for, measured in the working space before the output transform.".into()
        };
        cy += note(&painter, &cx.ctx, x, cy - 3.8, w, &text);
        cy - y
    }) + 12.0;

    let volume_size = &mut app.volume_size;
    let volume_id = app.volume_plot.id;
    let mut view_changed = false;
    y += card(cx, &mut child, x, y, w, "volume", &mut heights, |cx, ui, x, y, w| {
        let painter = ui.painter().clone();
        heading(&painter, pos2(x, y), "Colour volume · Oklab");
        let resp = tick(cx, ui, Id::new("volume-switch"), pos2(x + w - 11.0, y), d.volume, false);
        if resp.clicked() {
            d.volume = !d.volume;
            overlays_changed = true;
        }
        let cy = y + 15.0 + 5.6;
        let plot = Rect::from_min_size(pos2(x, cy), vec2(w, w));
        *volume_size = (plot.width() * cx.ppp, plot.height() * cx.ppp);
        let alpha = if d.volume { 1.0 } else { 0.25 };
        painter.rect_filled(plot, 6.0, Color32::BLACK);
        if let (Some(id), true) = (volume_id, has_image) {
            painter.add(egui::epaint::RectShape::filled(plot, 6.0, super::theme::fade(Color32::WHITE, alpha)).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
        }
        if !d.volume {
            painter.text(plot.center(), Align2::CENTER_CENTER, "off", mono(SIZE), white(0.35));
        }
        // Dragging turns the box.
        let resp = ui.interact(plot, Id::new("volume-turn"), if d.volume { cx.sense(Sense::drag()) } else { Sense::hover() });
        if resp.hovered() && d.volume {
            cx.ctx.set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        if resp.dragged() {
            let delta = resp.drag_delta() * 0.011;
            d.yaw += delta.x;
            d.pitch = (d.pitch + delta.y).clamp(-1.45, 1.45);
            view_changed = true;
        }
        let ay = plot.bottom() + 4.8;
        painter.text(pos2(x, ay), Align2::LEFT_TOP, "L ↑ · a/b", mono(SIZE), white(0.4));
        if d.volume {
            painter.text(pos2(x + w, ay), Align2::RIGHT_TOP, "drag to turn", mono(SIZE), white(0.4));
        }
        ay + 15.2 - y
    }) + 12.0;

    let report = app.renderer.report.clone();
    let counters = app.renderer.counters();
    let stream = app.debug.stream;
    y += card(cx, &mut child, x, y, w, "preview", &mut heights, |cx, ui, x, y, w| preview_card(cx, ui, x, y, w, report.as_ref(), counters, stream)) + 24.0;

    let content = y - start;
    let max_scroll = (content - bar.height()).max(0.0);
    if child.rect_contains_pointer(bar) {
        app.colour_bar_scroll = (scroll - cx.ctx.input(|i| i.smooth_scroll_delta.y)).clamp(0.0, max_scroll);
    }
    app.card_heights = heights;
    if overlays_changed {
        app.colour_space_debug.stats = None;
        app.colour_space_dirty = true;
        app.overlays_dirty = true;
    }
    if view_changed {
        app.colour_space_view_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn preview_card(cx: &mut Cx, ui: &mut Ui, x: f32, y: f32, w: f32, info: Option<&crate::gpu::renderer::PreviewState>, counters: crate::gpu::renderer::PreviewCounters, stream: Option<(u32, u32)>) -> f32 {
    let painter = ui.painter().clone();
    heading(&painter, pos2(x, y), "Preview · regions and levels");
    let mut cy = y + 15.0 + 5.6;
    let Some(info) = info else {
        cy += note(&painter, &cx.ctx, x, cy - 8.0, w, "Open an image to see what the preview holds.");
        return cy - y;
    };
    // A map of the image, with what the chain holds drawn over it.
    let pad = info.full_width.max(info.full_height) * 0.04;
    let (vw, vh) = (info.full_width + 2.0 * pad, info.full_height + 2.0 * pad);
    let map_h = (w as f64 * vh / vw).min(220.0) as f32;
    let map = Rect::from_min_size(pos2(x, cy), vec2(w, map_h));
    painter.rect_filled(map, 6.0, Color32::BLACK);
    let k = (map.width() as f64 / vw).min(map.height() as f64 / vh);
    let off = pos2(map.center().x - (vw * k) as f32 / 2.0, map.center().y - (vh * k) as f32 / 2.0);
    let to_map = |b: BoxF| Rect::from_min_max(off + vec2(((b[0] + pad) * k) as f32, ((b[1] + pad) * k) as f32), off + vec2(((b[2] + pad) * k) as f32, ((b[3] + pad) * k) as f32));
    let stroke = 1.0;
    painter.rect_filled(to_map([0.0, 0.0, info.full_width, info.full_height]), 0.0, white(0.06));
    let in_image = |r: &ChainRegion, b: BoxF| [b[0] / r.scale, b[1] / r.scale, b[2] / r.scale, b[3] / r.scale];
    if let Some(b) = &info.backdrop {
        let r = to_map(in_image(b, valid_box(b)));
        if info.backdrop_shown {
            painter.rect_filled(r, 0.0, rgba(240, 180, 90, 0.12));
        }
        painter.rect_stroke(r, 0.0, Stroke::new(stroke, rgba(240, 180, 90, 0.55)), egui::StrokeKind::Middle);
    }
    if let Some(d) = &info.detail {
        let whole = to_map(in_image(d, d.bx));
        painter.extend(egui::Shape::dashed_line(&[whole.left_top(), whole.right_top(), whole.right_bottom(), whole.left_bottom(), whole.left_top()], Stroke::new(stroke, rgba(110, 210, 200, 0.6)), 3.0, 2.0));
        let valid = to_map(in_image(d, valid_box(d)));
        painter.rect_filled(valid, 0.0, rgba(110, 210, 200, 0.22));
        painter.rect_stroke(valid, 0.0, Stroke::new(stroke, rgba(110, 210, 200, 0.9)), egui::StrokeKind::Middle);
    }
    if let Some(v) = info.visible {
        painter.rect_stroke(to_map(v), 0.0, Stroke::new(1.5, white(0.9)), egui::StrokeKind::Middle);
    }
    cy = map.bottom() + 8.8;
    painter.rect_filled(Rect::from_min_size(pos2(x, cy - 1.0), vec2(w, 1.0)), 0.0, white(0.1));
    cy += 7.2;
    let scale = if info.scale >= 1.0 { format!("{:.2}×", info.scale) } else { format!("{}%", (info.scale * 100.0).round()) };
    let detail = info.detail.map(|d| format!("{}% · apron {}", (d.scale * 100.0).round(), d.apron)).unwrap_or("none".into());
    let backdrop = info.backdrop.map(|b| format!("{}%{}", (b.scale * 100.0).round(), if info.backdrop_shown { " · shown" } else { "" })).unwrap_or("none".into());
    for (swatch, label, value) in [(0, "On screen", scale), (1, "Sharp region", detail), (2, "Backdrop", backdrop)] {
        let s = Rect::from_min_size(pos2(x, cy + 3.0), vec2(9.0, 9.0));
        match swatch {
            0 => {
                painter.rect_stroke(s, 2.0, Stroke::new(1.5, white(0.9)), egui::StrokeKind::Inside);
            }
            1 => {
                painter.rect_filled(s, 2.0, rgba(110, 210, 200, 0.6));
            }
            _ => {
                if info.backdrop_shown {
                    painter.rect_filled(s, 2.0, rgba(240, 180, 90, 0.4));
                }
                painter.rect_stroke(s, 2.0, Stroke::new(1.5, rgba(240, 180, 90, 0.7)), egui::StrokeKind::Inside);
            }
        }
        painter.text(pos2(x + 16.2, cy + 7.6), Align2::LEFT_CENTER, label, mono(SIZE), white(0.78));
        painter.text(pos2(x + w, cy + 7.6), Align2::RIGHT_CENTER, value, mono(SIZE), white(0.45));
        cy += 15.2 + 3.8;
    }

    // The mip levels, and which ones each region reads.
    cy += 5.6;
    painter.rect_filled(Rect::from_min_size(pos2(x, cy - 1.0), vec2(w, 1.0)), 0.0, white(0.1));
    cy += 7.2;
    let (sw, sh, levels) = info.source;
    let weights = |r: Option<&ChainRegion>| -> Vec<(u32, f64)> {
        let Some(r) = r else { return Vec::new() };
        let shrink = sw as f64 / info.full_width / r.scale;
        let lod = shrink.log2().clamp(0.0, levels as f64 - 1.0);
        let lo = lod.floor();
        let f = lod - lo;
        let mut out = vec![(lo as u32, 1.0 - f)];
        if f > 1e-3 {
            out.push((lo as u32 + 1, f));
        }
        out
    };
    let sharp = weights(info.detail.as_ref());
    let back = weights(info.backdrop.as_ref());
    let used = |l: u32| sharp.iter().chain(&back).any(|(k, _)| *k == l);
    let mut folded = 0;
    for l in 0..levels {
        if l >= 6 && !used(l) {
            folded += 1;
            continue;
        }
        let c = if used(l) { white(0.85) } else { white(0.4) };
        painter.text(pos2(x, cy + 7.6), Align2::LEFT_CENTER, format!("L{l}"), mono(SIZE), c);
        painter.text(pos2(x + 26.0, cy + 7.6), Align2::LEFT_CENTER, format!("{}×{}", (sw >> l).max(1), (sh >> l).max(1)), mono(SIZE), c);
        let mut right = x + w;
        for (list, label, bg, fg) in [(&back, "backdrop", rgba(240, 180, 90, 0.18), rgba(250, 210, 150, 0.95)), (&sharp, "sharp", rgba(110, 210, 200, 0.22), rgba(160, 235, 228, 0.95))] {
            if let Some((_, wgt)) = list.iter().find(|(k, _)| *k == l) {
                let text = format!("{label} {}%", (wgt * 100.0).round());
                let tw = text_width(&cx.ctx, &text, mono(SIZE)) + 7.4;
                let chip = Rect::from_min_size(pos2(right - tw, cy + 0.5), vec2(tw, 14.2));
                painter.rect_filled(chip, 3.0, bg);
                painter.text(chip.center(), Align2::CENTER_CENTER, text, mono(SIZE), fg);
                right -= tw + 2.0;
            }
        }
        cy += 15.2 + 3.8;
    }
    if folded > 0 {
        painter.text(pos2(x, cy + 7.6), Align2::LEFT_CENTER, format!("+{folded} smaller, unused"), mono(SIZE), white(0.4));
        cy += 15.2 + 3.8;
    }
    let source = format!(
        "Source {sw}×{sh}{}",
        match stream {
            Some((rows, total)) if rows < total => format!(" · streaming {}%", (rows as f64 / total as f64 * 100.0).round()),
            _ if (sw as f64) < info.full_width => " · preview only".into(),
            _ => " · full resolution".into(),
        }
    );
    let text = format!(
        "{source}\nSharp renders {} · last {:.1} ms\nBackdrop renders {} · put off {}",
        counters.detail_renders, counters.last_detail_ms, counters.backdrop_renders, counters.deferred
    );
    cy += note(&painter, &cx.ctx, x, cy - 3.8, w, &text);
    let _ = black(0.0);
    cy - y
}
