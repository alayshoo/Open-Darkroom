// src-native/src/ui/canvas.rs
//
// The preview: where the image sits and how the pointer moves it, the icon
// column with the crop and zoom controls, and the crop tool's overlay.

use egui::{pos2, vec2, Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, Ui};

use super::anim::EASE;
use super::cx::Cx;
use super::geo::Geo;
use super::path::{rect_path, Placed};
use super::theme::{black, fade, font, Weight};
use super::widgets::glass_button;
use crate::app::App;
use crate::model::crop::{crop_inside, fit_crop, is_full_frame, Crop, MAX_ANGLE};
use crate::model::viewport::{crop_layout, whole_layout, zoom_about, Layout, Slot, MAX_ZOOM};

/// Where the image sits on the canvas, in physical pixels, or None without an image.
pub fn layout(app: &App, geo: &Geo, ppp: f32) -> Option<Layout> {
    let img = app.image.as_ref()?;
    let ppp = ppp as f64;
    let c = geo.center;
    let slot = Slot {
        center: [c.center().x as f64 * ppp, c.center().y as f64 * ppp],
        width: c.width() as f64 * ppp,
        height: c.height() as f64 * ppp,
    };
    Some(if app.is_cropping {
        whole_layout(&app.draft_crop, img.width, img.height, &slot)
    } else {
        crop_layout(&app.crop, img.width, img.height, &slot, app.zoom, app.zoom_center)
    })
}

fn is_fit(scale: f64, fit: f64) -> bool {
    scale <= fit * 1.0001
}

/// A zoom no closer than the fit is the fit, however it was asked for.
pub fn settle_zoom(app: &mut App, layout: Option<&Layout>) {
    if app.zoom.is_none() {
        app.zoom_center = None;
    }
    let Some(l) = layout else {
        app.zoomable = false;
        return;
    };
    if let Some(z) = app.zoom {
        if !app.is_cropping && is_fit(z, l.fit) {
            app.zoom = None;
            app.zoom_center = None;
        }
    }
    app.zoomable = !app.is_cropping && !is_fit(1.0, l.fit);
}

/// Zoom, pinch and pan over the image, wherever no panel covers it.
pub fn preview_input(app: &mut App, cx: &mut Cx, ui: &Ui, layout: Option<&Layout>) {
    let screen = cx.ctx.screen_rect();
    let panning = app.zoom.is_some() && !app.is_cropping;
    let resp = ui.interact(screen, Id::new("preview"), if panning { Sense::click_and_drag() } else { Sense::hover() });
    let Some(l) = layout else { return };
    let ppp = cx.ppp as f64;
    let zoom_to = |app: &mut App, scale: f64, cursor: [f64; 2]| {
        if !scale.is_finite() {
            return;
        }
        if is_fit(scale, l.fit) {
            app.zoom = None;
            return;
        }
        let next = scale.min(MAX_ZOOM);
        app.zoom_center = Some(zoom_about(&l.placement, next, cursor));
        app.zoom = Some(next);
    };
    let pan_by = |app: &mut App, dx: f64, dy: f64| {
        let [cx_, cy] = l.placement.center;
        let s = l.placement.scale;
        app.zoom_center = Some([cx_ - dx / s, cy - dy / s]);
    };

    if panning {
        if resp.hovered() || resp.dragged() {
            cx.ctx.set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            pan_by(app, d.x as f64 * ppp, d.y as f64 * ppp);
        }
    }
    if !resp.hovered() || app.is_cropping {
        return;
    }
    let cursor = cx.ctx.input(|i| i.pointer.hover_pos()).map(|p| [p.x as f64 * ppp, p.y as f64 * ppp]).unwrap_or(l.placement.anchor);
    let events = cx.ctx.input(|i| i.events.clone());
    for e in events {
        match e {
            // A notch of a mouse wheel with Ctrl, or a trackpad pinch, is a
            // doubling per hundred pixels of travel.
            egui::Event::MouseWheel { unit, delta, modifiers, .. } => {
                let lines = if unit == egui::MouseWheelUnit::Line { 100.0 } else if unit == egui::MouseWheelUnit::Page { 800.0 } else { 1.0 };
                if modifiers.ctrl || modifiers.mac_cmd {
                    let stops = (delta.y as f64 * lines).clamp(-100.0, 100.0) / 100.0;
                    zoom_to(app, l.placement.scale * 2f64.powf(stops), cursor);
                } else if panning {
                    let factor = if unit == egui::MouseWheelUnit::Line { 33.0 } else { 1.0 };
                    pan_by(app, delta.x as f64 * factor * ppp, delta.y as f64 * factor * ppp);
                }
            }
            egui::Event::Zoom(factor) => {
                zoom_to(app, l.placement.scale * factor as f64, cursor);
            }
            _ => {}
        }
    }
}

const CROP_ICON: &str = "M8 4.5V16H19.5M4.5 8H16V19.5";
const TURN_ARROW: &str = "M19.5 14.5A7.5 7.5 0 0 0 12 7M14.1 4.9 12 7 14.1 9.1";
const RULER_TICKS: &str = "M6 9.5V11.5M9 9.5V11M12 9.5V12M15 9.5V11M18 9.5V11.5";
const REVERT_ARC: &str = "M12.7 7.56A4.5 4.5 0 1 1 7.57 12.78";
const REVERT_HEAD: &str = "M9.15 7.5 12 5.35 12 9.65Z";

/// An icon drawn the way the toolbar's are: centred in the button, inked like
/// the title bar, with a soft shadow under it.
fn glyph(painter: &egui::Painter, rect: Rect, size: f32, view: f32, scale: f32, ink: Color32, draw: impl Fn(&egui::Painter, &Placed, Color32, f32)) {
    let s = size / view * scale;
    for (offset, colour) in [(vec2(0.0, 1.0), black(0.4)), (vec2(0.0, 0.0), ink)] {
        let placed = Placed::new(rect.center() - vec2(size, size) * scale / 2.0 + offset, s);
        draw(painter, &placed, colour, s);
    }
}

pub fn toolbar(app: &mut App, cx: &mut Cx, ui: &mut Ui, geo: &Geo) {
    super::sidebar::block(ui, Id::new("toolbar"), Rect::from_min_size(pos2(0.0, 50.0), vec2(42.0, 192.0)));
    let has_image = app.image.is_some();
    let dim = cx.anim.value("toolbar-dim", if has_image { 1.0 } else { 0.25 }, 0.2, EASE);
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= dim;
    cx.enabled &= has_image;
    let ink = cx.pal.chrome_ink;

    let crop = Rect::from_min_size(pos2(1.0, 50.0), vec2(40.0, 40.0));
    let resp = glass_button(cx, ui, Id::new("crop"), crop, app.is_cropping, false, |p, r, s| {
        glyph(p, r, 40.0, 30.0, s, ink, |p, placed, c, k| {
            let mut placed = Placed::new(placed.origin, placed.scale);
            placed.view_origin = vec2(-3.0, -3.0);
            placed.stroke(p, CROP_ICON, Stroke::new(1.05 * k, c));
        });
    });
    if resp.clicked() {
        app.toggle_crop();
    }

    // The crop tools stay mounted and fade, so the column never jumps.
    let tools = cx.anim.flag("crop-tools", app.is_cropping, 0.25, EASE);
    if tools > 0.0 {
        let s2 = (cx.opacity, cx.enabled);
        cx.opacity *= tools;
        cx.enabled &= app.is_cropping;
        let at = |i: usize| Rect::from_min_size(pos2(4.0, 94.0 + i as f32 * 38.0), vec2(34.0, 34.0));
        for (i, clockwise) in [(0usize, false), (1, true)] {
            let resp = glass_button(cx, ui, Id::new(("turn", i)), at(i), false, false, |p, r, s| {
                glyph(p, r, 28.0, 24.0, s, ink, |p, placed, c, k| {
                    let mut placed = Placed::new(placed.origin, placed.scale);
                    if clockwise {
                        placed.mirror_width = Some(24.0);
                    }
                    placed.stroke(p, &rect_path(4.5, 11.0, 10.0, 8.5, 1.2), Stroke::new(1.2 * k, c));
                    placed.stroke(p, TURN_ARROW, Stroke::new(1.2 * k, c));
                });
            });
            if resp.clicked() {
                app.turn_draft(clockwise);
            }
        }
        let resp = glass_button(cx, ui, Id::new("straighten"), at(2), app.is_straightening, false, |p, r, s| {
            glyph(p, r, 28.0, 24.0, s, ink, |p, placed, c, k| {
                let mut placed = Placed::new(placed.origin, placed.scale);
                placed.rotate = Some((-24.0, pos2(12.0, 12.0)));
                placed.stroke(p, &rect_path(2.5, 9.5, 19.0, 5.0, 1.0), Stroke::new(1.2 * k, c));
                placed.stroke(p, RULER_TICKS, Stroke::new(1.0 * k, c));
            });
        });
        if resp.clicked() {
            app.is_straightening = !app.is_straightening;
        }
        let can_reset = !is_full_frame(&app.draft_crop);
        let resp = glass_button(cx, ui, Id::new("crop-reset"), at(3), false, !can_reset, |p, r, s| {
            glyph(p, r, 28.0, 24.0, s, ink, |p, placed, c, k| {
                placed.stroke(p, REVERT_ARC, Stroke::new(1.2 * k, c));
                placed.fill(p, REVERT_HEAD, c);
            });
        });
        if resp.clicked() {
            app.draft_crop = crate::model::crop::FULL_FRAME;
        }
        (cx.opacity, cx.enabled) = s2;
    }

    // Fit / 1:1, at the foot of the column.
    let zoom = Rect::from_min_size(pos2(1.0, geo.screen.bottom() - 40.0), vec2(40.0, 40.0));
    super::sidebar::block(ui, Id::new("toolbar-zoom"), zoom);
    let fitted = app.zoom.is_none();
    let disabled = app.is_cropping || (fitted && !app.zoomable);
    let label = if fitted { "1:1" } else { "Fit" };
    let resp = glass_button(cx, ui, Id::new("zoom"), zoom, false, disabled, |p, r, s| {
        let f = font(12.0 * s, Weight::SemiBold);
        p.text(r.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, label, f.clone(), black(0.75));
        p.text(r.center(), Align2::CENTER_CENTER, label, f, ink);
    });
    if resp.clicked() {
        app.toggle_zoom();
    }
    (cx.opacity, cx.enabled) = saved;
}

// ── The crop overlay ──────────────────────────────────────────────────────────

/// Smallest the crop may get along either axis, as a fraction of the frame.
const MIN_EXTENT: f64 = 0.02;
/// Shorter straighten lines than this, in points, are taken as a slip.
const MIN_LINE: f32 = 8.0;

#[derive(Clone, Copy, PartialEq)]
struct Sides {
    l: bool,
    r: bool,
    t: bool,
    b: bool,
}

const MOVE: Sides = Sides { l: true, r: true, t: true, b: true };

#[derive(Clone, Copy)]
enum Drag {
    Resize { sides: Sides, from: Pos2, start: Crop },
    Rotate { pivot: Pos2, bearing: f32, start: Crop },
    Line { from: [f32; 2], to: [f32; 2] },
}

fn cursor_for(s: Sides) -> CursorIcon {
    if s == MOVE {
        CursorIcon::Move
    } else if (s.l && s.t) || (s.r && s.b) {
        CursorIcon::ResizeNwSe
    } else if (s.r && s.t) || (s.l && s.b) {
        CursorIcon::ResizeNeSw
    } else if s.t || s.b {
        CursorIcon::ResizeVertical
    } else {
        CursorIcon::ResizeHorizontal
    }
}

fn to_rect(c: &Crop) -> [f64; 4] {
    [c.cx - c.w / 2.0, c.cy - c.h / 2.0, c.cx + c.w / 2.0, c.cy + c.h / 2.0]
}

fn from_rect(r: [f64; 4], base: &Crop) -> Crop {
    Crop { cx: (r[0] + r[2]) / 2.0, cy: (r[1] + r[3]) / 2.0, w: r[2] - r[0], h: r[3] - r[1], ..*base }
}

pub fn crop_overlay(app: &mut App, cx: &mut Cx, ui: &Ui, layout: &Layout) {
    let Some(img) = &app.image else { return };
    let (iw, ih) = (img.width as f64, img.height as f64);
    let ppp = cx.ppp;
    let size = vec2(layout.frame[0] as f32 / ppp, layout.frame[1] as f32 / ppp);
    let anchor = pos2(layout.placement.anchor[0] as f32 / ppp, layout.placement.anchor[1] as f32 / ppp);
    let frame = Rect::from_center_size(anchor, size);

    // On the image, and within the upright frame the overlay is laid out on.
    let inside = |c: &Crop| {
        crop_inside(c, iw, ih) && c.cx - c.w / 2.0 >= -1e-9 && c.cy - c.h / 2.0 >= -1e-9 && c.cx + c.w / 2.0 <= 1.0 + 1e-9 && c.cy + c.h / 2.0 <= 1.0 + 1e-9
    };
    let limited = |from: &Crop, to: &Crop| -> Crop {
        if inside(to) {
            return *to;
        }
        let at = |t: f64| Crop {
            cx: from.cx + (to.cx - from.cx) * t,
            cy: from.cy + (to.cy - from.cy) * t,
            w: from.w + (to.w - from.w) * t,
            h: from.h + (to.h - from.h) * t,
            ..*to
        };
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if inside(&at(mid)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        at(lo)
    };
    let fraction = |p: Pos2| [(p.x - frame.left()) / frame.width(), (p.y - frame.top()) / frame.height()];

    let crop = app.draft_crop;
    let r = to_rect(&crop);
    let box_rect = Rect::from_min_max(
        pos2(frame.left() + r[0] as f32 * size.x, frame.top() + r[1] as f32 * size.y),
        pos2(frame.left() + r[2] as f32 * size.x, frame.top() + r[3] as f32 * size.y),
    );

    let drag_id = Id::new("crop-drag");
    let drag: Option<Drag> = cx.ctx.data(|d| d.get_temp(drag_id));
    let pointer = cx.ctx.input(|i| i.pointer.interact_pos());
    let straightening = app.is_straightening || cx.ctx.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);

    // Hit regions: anywhere turns the image, the box moves it, the handles
    // resize it. Registered back to front, so the handles win.
    let rotate_zone = ui.interact(ui.clip_rect(), Id::new("crop-rotate"), Sense::drag());
    let box_resp = ui.interact(box_rect, Id::new("crop-box"), Sense::drag());
    let mut handles = Vec::new();
    let corner = |x: f32, y: f32| Rect::from_center_size(pos2(x, y), vec2(24.0, 24.0));
    let b = box_rect;
    let specs: [(&str, Sides, Rect); 8] = [
        ("nw", Sides { l: true, r: false, t: true, b: false }, corner(b.left(), b.top())),
        ("n", Sides { l: false, r: false, t: true, b: false }, Rect::from_min_max(pos2(b.left() + 24.0, b.top() - 12.0), pos2(b.right() - 24.0, b.top() + 12.0))),
        ("ne", Sides { l: false, r: true, t: true, b: false }, corner(b.right(), b.top())),
        ("e", Sides { l: false, r: true, t: false, b: false }, Rect::from_min_max(pos2(b.right() - 12.0, b.top() + 24.0), pos2(b.right() + 12.0, b.bottom() - 24.0))),
        ("se", Sides { l: false, r: true, t: false, b: true }, corner(b.right(), b.bottom())),
        ("s", Sides { l: false, r: false, t: false, b: true }, Rect::from_min_max(pos2(b.left() + 24.0, b.bottom() - 12.0), pos2(b.right() - 24.0, b.bottom() + 12.0))),
        ("sw", Sides { l: true, r: false, t: false, b: true }, corner(b.left(), b.bottom())),
        ("w", Sides { l: true, r: false, t: false, b: false }, Rect::from_min_max(pos2(b.left() - 12.0, b.top() + 24.0), pos2(b.left() + 12.0, b.bottom() - 24.0))),
    ];
    for (name, sides, rect) in specs {
        handles.push((ui.interact(rect, Id::new(("crop-handle", name)), Sense::drag()), sides));
    }

    if drag.is_none() {
        let start_line = |p: Pos2| Drag::Line { from: fraction(p), to: fraction(p) };
        let mut started = None;
        for (resp, sides) in handles.iter().rev() {
            if resp.hovered() {
                cx.ctx.set_cursor_icon(if straightening { CursorIcon::Crosshair } else { cursor_for(*sides) });
            }
            if resp.drag_started() {
                started = Some(pointer.map(|p| if straightening { start_line(p) } else { Drag::Resize { sides: *sides, from: p, start: crop } }));
                break;
            }
        }
        if started.is_none() {
            if box_resp.hovered() && !handles.iter().any(|(r, _)| r.hovered()) {
                cx.ctx.set_cursor_icon(if straightening { CursorIcon::Crosshair } else { CursorIcon::Move });
            }
            if box_resp.drag_started() {
                started = Some(pointer.map(|p| if straightening { start_line(p) } else { Drag::Resize { sides: MOVE, from: p, start: crop } }));
            } else if rotate_zone.hovered() && !box_resp.hovered() && !handles.iter().any(|(r, _)| r.hovered()) {
                cx.ctx.set_cursor_icon(if straightening { CursorIcon::Crosshair } else { CursorIcon::Grab });
            }
            if started.is_none() && rotate_zone.drag_started() {
                let pivot = pos2(frame.left() + crop.cx as f32 * size.x, frame.top() + crop.cy as f32 * size.y);
                started = Some(pointer.map(|p| {
                    if straightening {
                        start_line(p)
                    } else {
                        Drag::Rotate { pivot, bearing: (p.y - pivot.y).atan2(p.x - pivot.x), start: crop }
                    }
                }));
            }
        }
        if let Some(Some(d)) = started {
            cx.ctx.data_mut(|data| data.insert_temp(drag_id, d));
        }
    }

    let drag: Option<Drag> = cx.ctx.data(|d| d.get_temp(drag_id));
    let down = cx.ctx.input(|i| i.pointer.primary_down());
    if let (Some(d), Some(p)) = (drag, pointer) {
        match d {
            Drag::Line { from, .. } => {
                cx.ctx.data_mut(|data| data.insert_temp(drag_id, Drag::Line { from, to: fraction(p) }));
                cx.ctx.set_cursor_icon(CursorIcon::Crosshair);
            }
            Drag::Rotate { pivot, bearing, start } => {
                let mut turn = (p.y - pivot.y).atan2(p.x - pivot.x) - bearing;
                turn = turn.sin().atan2(turn.cos());
                let angle = (start.angle + turn as f64).clamp(-MAX_ANGLE, MAX_ANGLE);
                app.draft_crop = fit_crop(&Crop { angle, ..start }, iw, ih);
                cx.ctx.set_cursor_icon(CursorIcon::Grabbing);
            }
            Drag::Resize { sides, from, start } => {
                let dx = ((p.x - from.x) / size.x) as f64;
                let dy = ((p.y - from.y) / size.y) as f64;
                let s = to_rect(&start);
                // Each axis is limited on its own, so a box pinned on one side
                // can still slide along the other.
                let next = if sides == MOVE {
                    let across = limited(&start, &Crop { cx: start.cx + dx, ..start });
                    limited(&across, &Crop { cy: across.cy + dy, ..across })
                } else {
                    let mut x = s;
                    if sides.l {
                        x[0] = (s[0] + dx).clamp(0.0, s[2] - MIN_EXTENT);
                    }
                    if sides.r {
                        x[2] = (s[2] + dx).clamp(s[0] + MIN_EXTENT, 1.0);
                    }
                    let across = limited(&start, &from_rect(x, &start));
                    let mut y = to_rect(&across);
                    if sides.t {
                        y[1] = (s[1] + dy).clamp(0.0, y[3] - MIN_EXTENT);
                    }
                    if sides.b {
                        y[3] = (s[3] + dy).clamp(y[1] + MIN_EXTENT, 1.0);
                    }
                    limited(&across, &from_rect(y, &across))
                };
                app.draft_crop = next;
                cx.ctx.set_cursor_icon(cursor_for(sides));
            }
        }
        if !down {
            // A finished line is turned level, or plumb when nearer upright.
            if let Drag::Line { from, to } = d {
                let dx = (to[0] - from[0]) * size.x;
                let dy = (to[1] - from[1]) * size.y;
                if dx.hypot(dy) >= MIN_LINE {
                    let bearing = dy.atan2(dx) as f64;
                    let quarter = std::f64::consts::FRAC_PI_2;
                    let tilt = bearing - (bearing / quarter).round() * quarter;
                    let c = app.draft_crop;
                    let angle = (c.angle - tilt).clamp(-MAX_ANGLE, MAX_ANGLE);
                    app.draft_crop = fit_crop(&Crop { angle, ..c }, iw, ih);
                    app.is_straightening = false;
                }
            }
            cx.ctx.data_mut(|data| data.remove::<Drag>(drag_id));
        }
    }

    // Drawing.
    let drag: Option<Drag> = cx.ctx.data(|d| d.get_temp(drag_id));
    let rotating = matches!(drag, Some(Drag::Rotate { .. }));
    let dragging = drag.is_some();
    let crop = app.draft_crop;
    let r = to_rect(&crop);
    let b = Rect::from_min_max(
        pos2(frame.left() + r[0] as f32 * size.x, frame.top() + r[1] as f32 * size.y),
        pos2(frame.left() + r[2] as f32 * size.x, frame.top() + r[3] as f32 * size.y),
    );
    let painter = cx.painter(ui, b);
    let ink = cx.pal.color1;
    let mark = cx.fx.mark();
    cx.fx.ring(b, [0.0; 4], 1.0, fade(ink, 0.85));
    let grid_alpha = cx.anim.value("crop-grid", if dragging { 0.6 } else { 0.35 }, 0.15, EASE);
    let lines: Vec<f32> = if rotating { (1..8).map(|i| i as f32 / 8.0).collect() } else { vec![1.0 / 3.0, 2.0 / 3.0] };
    for at in lines {
        cx.fx.fill(Rect::from_min_size(pos2(b.left() + at * b.width(), b.top()), vec2(1.0, b.height())), [0.0; 4], fade(ink, grid_alpha));
        cx.fx.fill(Rect::from_min_size(pos2(b.left(), b.top() + at * b.height()), vec2(b.width(), 1.0)), [0.0; 4], fade(ink, grid_alpha));
    }
    // The handles: L-shaped corners and short bars on the edges.
    let marks: [Rect; 12] = [
        Rect::from_min_size(b.left_top() - vec2(2.0, 2.0), vec2(18.0 + 3.0 - 1.0, 3.0)),
        Rect::from_min_size(b.left_top() - vec2(2.0, 2.0), vec2(3.0, 20.0)),
        Rect::from_min_size(pos2(b.right() - 18.0, b.top() - 2.0), vec2(20.0, 3.0)),
        Rect::from_min_size(pos2(b.right() - 1.0, b.top() - 2.0), vec2(3.0, 20.0)),
        Rect::from_min_size(pos2(b.right() - 18.0, b.bottom() - 1.0), vec2(20.0, 3.0)),
        Rect::from_min_size(pos2(b.right() - 1.0, b.bottom() - 18.0), vec2(3.0, 20.0)),
        Rect::from_min_size(pos2(b.left() - 2.0, b.bottom() - 1.0), vec2(20.0, 3.0)),
        Rect::from_min_size(pos2(b.left() - 2.0, b.bottom() - 18.0), vec2(3.0, 20.0)),
        Rect::from_center_size(pos2(b.center().x, b.top() - 0.5), vec2(20.0, 3.0)),
        Rect::from_center_size(pos2(b.center().x, b.bottom() + 0.5), vec2(20.0, 3.0)),
        Rect::from_center_size(pos2(b.left() - 0.5, b.center().y), vec2(3.0, 20.0)),
        Rect::from_center_size(pos2(b.right() + 0.5, b.center().y), vec2(3.0, 20.0)),
    ];
    for m in marks {
        cx.fx.shadow(m, 0.0, &crate::gpu::chrome::shadow(0.0, 0.0, 3.0, 0.0, black(0.6)));
    }
    for m in marks {
        cx.fx.fill(m, [0.0; 4], ink);
    }
    cx.fx.flush(&painter, mark);

    if rotating {
        let text = format!("{:.1}°", crop.angle.to_degrees());
        let f = font(12.0, Weight::Medium);
        let w = super::cx::text_width(&cx.ctx, &text, f.clone()) + 16.0;
        let pill = Rect::from_min_size(pos2(b.center().x - w / 2.0, b.top() + 10.0), vec2(w, 20.0));
        let mark = cx.fx.mark();
        cx.fx.fill(pill, [6.0; 4], black(0.6));
        cx.fx.flush(&painter, mark);
        painter.text(pill.center(), Align2::CENTER_CENTER, text, f, ink);
    }
    if let Some(Drag::Line { from, to }) = drag {
        let a = pos2(frame.left() + from[0] * size.x, frame.top() + from[1] * size.y);
        let z = pos2(frame.left() + to[0] * size.x, frame.top() + to[1] * size.y);
        painter.line_segment([a, z], Stroke::new(3.0, black(0.35)));
        painter.line_segment([a, z], Stroke::new(1.5, ink));
    }
    let _ = Color32::TRANSPARENT;
}
