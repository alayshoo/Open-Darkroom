// src-native/src/ui/curve_editor.rs
//
// The point-curve editor: a channel picker, and a square plot over the
// histogram backdrop where points are added, dragged, and dragged away or
// double-clicked to remove. Mirrors CurveEditor.svelte.

use egui::{pos2, vec2, Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Shape, Stroke, Ui};

use super::cx::{shadow_text, Cx};
use super::sidebar::{swatch, Commits};
use super::theme::{fade, font, white, Weight};
use crate::app::App;
use crate::model::curves::{is_identity, Evaluator, Point, CHANNELS, COLOURS, MASTER};

const SAMPLES: usize = 128;
const GRID: [f32; 3] = [0.25, 0.5, 0.75];
/// Pointer distance that grabs a point.
const HIT_RADIUS: f32 = 8.0;
/// Closest two neighbouring points may sit, as a fraction of the axis.
const MIN_GAP: f64 = 0.01;
/// How far past the edge a dragged point has to go to be removed.
const REMOVE_DISTANCE: f32 = 24.0;

#[derive(Clone)]
struct Drag {
    channel: usize,
    index: usize,
    base: Vec<Point>,
    before: crate::model::curves::Curves,
}

fn colour(c: usize) -> Color32 {
    let [r, g, b] = COLOURS[c];
    Color32::from_rgb(r, g, b)
}

/// Draws the editor at `y`, `width` wide, and returns its height.
pub fn curve_editor(cx: &mut Cx, ui: &mut Ui, app: &mut App, x: f32, y: f32, width: f32, commits: &mut Commits) -> f32 {
    let rgb = app.is_rgb;
    let active = if rgb { app.curve_channel } else { MASTER };

    // Title and channel swatches.
    let row = Rect::from_min_size(pos2(x, y), vec2(width - 6.0, 24.0));
    let painter = cx.painter(ui, row);
    shadow_text(&painter, pos2(x, row.center().y), Align2::LEFT_CENTER, "Curves", font(18.0, Weight::SemiBold), cx.pal.color1, 0.35);
    for c in 0..CHANNELS {
        let center = pos2(row.right() - 6.0 - (CHANNELS - 1 - c) as f32 * 20.0, row.center().y);
        let adjusted = !is_identity(&app.curves.0[c]);
        if swatch(cx, ui, Id::new(("curve-channel", c)), center, colour(c), c == active, adjusted, !rgb && c != MASTER) {
            app.curve_channel = c;
        }
    }

    let plot = Rect::from_min_size(pos2(x, y + 24.0 + 6.0), vec2(width, width));
    app.curve_size = width * cx.ppp;
    let painter = cx.painter(ui, plot);
    let mark = cx.fx.mark();
    cx.fx.fill(plot, [6.0; 4], Color32::BLACK);
    cx.fx.flush(&painter, mark);
    if let (Some(id), true) = (app.curve_plot.id, app.image.is_some()) {
        let tint = fade(Color32::WHITE, 0.22 * painter.opacity());
        painter.add(egui::epaint::RectShape::filled(plot, 6.0, tint).with_texture(id, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
    }

    let at = |p: &Point| pos2(plot.left() + p.x as f32 * width, plot.top() + (1.0 - p.y as f32) * width);
    let grid = Stroke::new(1.0_f32, white(0.08));
    for g in GRID {
        painter.line_segment([pos2(plot.left() + g * width, plot.top()), pos2(plot.left() + g * width, plot.bottom())], grid);
        painter.line_segment([pos2(plot.left(), plot.top() + g * width), pos2(plot.right(), plot.top() + g * width)], grid);
    }
    let dashes = Shape::dashed_line(&[plot.left_bottom(), plot.right_top()], Stroke::new(1.0_f32, white(0.12)), 3.0, 3.0);
    painter.extend(dashes);

    // Inactive curves first, so the one being edited is drawn on top.
    let visible: Vec<usize> = if rgb { (0..CHANNELS).collect() } else { vec![MASTER] };
    let order: Vec<usize> = visible.iter().copied().filter(|&c| c != active).chain(std::iter::once(active)).collect();
    for c in order {
        let f = Evaluator::new(&app.curves.0[c]);
        let points: Vec<Pos2> = (0..=SAMPLES)
            .map(|i| {
                let t = i as f64 / SAMPLES as f64;
                at(&Point { x: t, y: f.eval(t) })
            })
            .collect();
        let (w, a): (f32, f32) = if c == active { (1.75, 1.0) } else { (1.25, 0.3) };
        painter.add(Shape::line(points, Stroke::new(w, fade(colour(c), a))));
    }
    for p in &app.curves.0[active] {
        let c = at(p);
        painter.circle(c, 4.0, Color32::BLACK, Stroke::new(1.5_f32, colour(active)));
    }

    // Editing.
    let id = Id::new("curve-plot");
    let resp = ui.interact(plot, id, cx.sense(Sense::click_and_drag()));
    let drag_id = id.with("drag");
    let drag: Option<Drag> = cx.ctx.data(|d| d.get_temp(drag_id));
    let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
    let to_curve = |p: Pos2| ((p.x - plot.left()) / width, 1.0 - (p.y - plot.top()) / width);
    let outside = |p: Pos2| (plot.left() - p.x).max(p.x - plot.right()).max(plot.top() - p.y).max(p.y - plot.bottom()).max(0.0);
    let hit = |points: &[Point], p: Pos2| {
        let mut best = None;
        let mut best_d = HIT_RADIUS;
        for (i, q) in points.iter().enumerate() {
            let d = (at(q) - p).length();
            if d <= best_d {
                best = Some(i);
                best_d = d;
            }
        }
        best
    };
    if resp.hovered() && cx.enabled {
        cx.ctx.set_cursor_icon(if drag.is_some() { CursorIcon::Grabbing } else { CursorIcon::Crosshair });
    }

    if drag.is_none() && resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
        if let Some(p) = pointer {
            let before = app.curves.clone();
            let mut points = before.0[active].clone();
            let index = match hit(&points, p) {
                Some(i) => Some(i),
                None => {
                    let (fx, fy) = to_curve(p);
                    let x = (fx as f64).clamp(0.0, 1.0);
                    match points.iter().position(|q| q.x > x) {
                        Some(at) if at > 0 && x - points[at - 1].x >= MIN_GAP && points[at].x - x >= MIN_GAP => {
                            points.insert(at, Point { x, y: (fy as f64).clamp(0.0, 1.0) });
                            app.curves.0[active] = points.clone();
                            Some(at)
                        }
                        _ => None,
                    }
                }
            };
            if let Some(index) = index {
                cx.ctx.data_mut(|d| d.insert_temp(drag_id, Drag { channel: active, index, base: points, before }));
            }
        }
    }

    if let (Some(d), Some(p)) = (cx.ctx.data(|d| d.get_temp::<Drag>(drag_id)), pointer) {
        let down = ui.input(|i| i.pointer.primary_down());
        if down {
            let mut points = d.base.clone();
            let i = d.index;
            let is_end = i == 0 || i == points.len() - 1;
            let (fx, fy) = to_curve(p);
            if !is_end && outside(p) > REMOVE_DISTANCE {
                points.remove(i);
            } else {
                if !is_end {
                    points[i].x = (fx as f64).clamp(points[i - 1].x + MIN_GAP, points[i + 1].x - MIN_GAP);
                }
                points[i].y = (fy as f64).clamp(0.0, 1.0);
            }
            app.curves.0[d.channel] = points;
        } else {
            cx.ctx.data_mut(|data| data.remove::<Drag>(drag_id));
            if d.before != app.curves {
                commits.curves = Some((d.before, app.curves.clone()));
            }
        }
    }

    if resp.double_clicked() {
        if let Some(p) = pointer {
            if let Some(i) = hit(&app.curves.0[active], p) {
                if i > 0 && i < app.curves.0[active].len() - 1 {
                    let before = app.curves.clone();
                    app.curves.0[active].remove(i);
                    commits.curves = Some((before, app.curves.clone()));
                }
            }
        }
    }

    24.0 + 6.0 + width
}
