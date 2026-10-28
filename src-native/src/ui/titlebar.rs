// src-native/src/ui/titlebar.rs
//
// The window's own title bar: the app glyph and the menus on the left, the
// title in the middle, the window controls on the right, all straight on the
// scrim, with the band itself dragging the window.

use egui::{pos2, vec2, Align2, Color32, CursorIcon, Id, Pos2, Rect, Sense, Stroke, Ui, ViewportCommand};

use super::anim::{EASE, EASE_OUT_EMPH};
use super::cx::{text_width, Cx};
use super::path::{capped_line, Placed};
use super::theme::{black, fade, font, mix, Weight};
use crate::app::App;
use crate::model::shortcuts::ShortcutId;

pub enum Item {
    Entry { label: &'static str, shortcut: String, action: Option<MenuAction> },
    Separator,
}

#[derive(Clone, Copy)]
pub enum MenuAction {
    Open,
    Settings,
    Undo,
    Redo,
    Copy,
    Paste,
    Reset,
    Shortcuts,
}

fn menus(app: &App) -> [(&'static str, Vec<Item>); 3] {
    let sc = |id| app.shortcuts.display(id);
    let e = |label, shortcut: String, action| Item::Entry { label, shortcut, action };
    [
        (
            "File",
            vec![
                e("Open…", sc(ShortcutId::OpenImage), Some(MenuAction::Open)),
                Item::Separator,
                e("Export…", "Ctrl+Shift+E".into(), None),
                Item::Separator,
                e("Settings", sc(ShortcutId::ToggleSettings), Some(MenuAction::Settings)),
            ],
        ),
        (
            "Edit",
            vec![
                e("Undo", sc(ShortcutId::Undo), Some(MenuAction::Undo)),
                e("Redo", sc(ShortcutId::Redo), Some(MenuAction::Redo)),
                Item::Separator,
                e("Copy Adjustments", sc(ShortcutId::CopyAdjustments), Some(MenuAction::Copy)),
                e("Paste Adjustments", sc(ShortcutId::PasteAdjustments), Some(MenuAction::Paste)),
                e("Reset Adjustments", sc(ShortcutId::ResetAdjustments), Some(MenuAction::Reset)),
            ],
        ),
        (
            "Help",
            vec![
                e("Documentation", String::new(), None),
                e("Keyboard Shortcuts", String::new(), Some(MenuAction::Shortcuts)),
                Item::Separator,
                e("Report a Bug…", String::new(), None),
                e("About Open Darkroom", String::new(), None),
            ],
        ),
    ]
}

/// Text with the title bar's legibility shadow, which stands in for the
/// scrim wherever the pixels underneath are bright.
fn ink_text(painter: &egui::Painter, pos: Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    painter.text(pos + vec2(0.0, 1.0), align, text, font(size, Weight::Regular), black(0.45));
    painter.text(pos, align, text, font(size, Weight::Regular), color);
}

pub fn titlebar(app: &mut App, cx: &mut Cx, ui: &mut Ui) -> Option<MenuAction> {
    let screen = cx.ctx.screen_rect();
    let band = Rect::from_min_size(Pos2::ZERO, vec2(screen.width(), 38.0));
    let mut action = None;

    // The drag layer, under the controls.
    let drag = ui.interact(band, Id::new("titlebar-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        cx.ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let maximized = cx.ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        cx.ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }

    let modal = cx.anim.value("titlebar-modal", if app.settings_open { 0.35 } else { 1.0 }, 0.18, EASE);

    // The app glyph, inked like the menus.
    let icon = Rect::from_min_size(pos2(12.0, 8.0), vec2(22.0, 22.0));
    let painter = ui.painter().clone();
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    painter.image(app.icons.app.id(), icon.translate(vec2(0.0, 1.0)), uv, black(0.45));
    painter.image(app.icons.app.id(), icon, uv, cx.pal.color2);

    // Menus.
    let entries = menus(app);
    let mut x = 44.0;
    let saved = (cx.opacity, cx.enabled);
    cx.opacity *= modal;
    cx.enabled &= !app.settings_open;
    let mut button_rects = Vec::new();
    for (i, (label, _)) in entries.iter().enumerate() {
        let w = text_width(&cx.ctx, label, font(15.0, Weight::Regular)) + 16.0;
        let r = Rect::from_min_size(pos2(x, 5.0), vec2(w, 28.0));
        button_rects.push(r);
        let id = Id::new(("menu", i));
        let resp = ui.interact(r, id, cx.sense(Sense::click()));
        let open = app.menu_open == Some(i);
        let lit = cx.anim.flag(id.with("lit"), resp.hovered() || open, 0.2, EASE_OUT_EMPH);
        let painter = cx.painter(ui, r);
        if lit > 0.0 {
            let mark = cx.fx.mark();
            cx.fx.fill(r, [7.0; 4], fade(cx.pal.bg5, lit));
            cx.fx.flush(&painter, mark);
        }
        ink_text(&painter, r.center(), Align2::CENTER_CENTER, label, 15.0, mix(cx.pal.color2, cx.pal.color1, lit));
        if resp.clicked() {
            app.menu_switching = false;
            app.menu_open = if open { None } else { Some(i) };
            if app.menu_open.is_some() {
                cx.anim.restart(Id::new("menu-in"), 0.0, 1.0, 0.15, EASE_OUT_EMPH);
            }
        } else if resp.hovered() && app.menu_open.is_some() && !open {
            // A hover only takes over while the bar is already open.
            app.menu_switching = true;
            app.menu_open = Some(i);
        }
        x = r.right() + 4.0;
    }

    // The title.
    let painter = cx.painter(ui, band);
    painter.text(pos2(screen.center().x, 19.0 + 1.0), Align2::CENTER_CENTER, &app.title, font(15.0, Weight::Regular), black(0.75));
    painter.text(pos2(screen.center().x, 19.0), Align2::CENTER_CENTER, &app.title, font(15.0, Weight::Regular), cx.pal.color1);
    (cx.opacity, cx.enabled) = saved;

    // Window controls.
    let maximized = cx.ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let right = screen.right() - 5.0;
    let controls = [
        (right - 36.0 * 3.0 - 4.0, "min"),
        (right - 36.0 * 2.0 - 2.0, "max"),
        (right - 36.0, "close"),
    ];
    for (left, kind) in controls {
        let r = Rect::from_min_size(pos2(left, 5.0), vec2(36.0, 28.0));
        let id = Id::new(("window", kind));
        let resp = ui.interact(r, id, Sense::click());
        let lit = cx.anim.flag(id.with("lit"), resp.hovered(), 0.2, EASE_OUT_EMPH);
        let painter = cx.painter(ui, r);
        if lit > 0.0 {
            let mark = cx.fx.mark();
            cx.fx.fill(r, [7.0; 4], fade(cx.pal.bg5, lit));
            cx.fx.flush(&painter, mark);
        }
        let ink = mix(cx.pal.color2, cx.pal.color1, lit);
        for (offset, colour) in [(vec2(0.0, 1.0), black(0.5)), (vec2(0.0, 0.0), ink)] {
            let c = r.center() + offset;
            match kind {
                "min" => capped_line(&painter, c - vec2(8.2, 0.0), c + vec2(8.2, 0.0), 1.6, colour),
                "max" if !maximized => {
                    let s = 17.0 / 24.0;
                    let rr = Rect::from_min_size(c + vec2(-7.5, -6.5) * s * 1.0 + vec2(0.0, 0.5), vec2(15.0, 13.0) * s);
                    painter.rect_stroke(rr, 1.5 * s, Stroke::new(2.0 * s, colour), egui::StrokeKind::Middle);
                }
                "max" => {
                    let s = 20.0 / 24.0;
                    let o = c - vec2(12.0, 12.0) * s;
                    let back = Rect::from_min_max(o + vec2(8.5, 5.25) * s, o + vec2(20.25, 15.3) * s);
                    let front = Rect::from_min_max(o + vec2(4.85, 8.8) * s, o + vec2(16.65, 18.75) * s);
                    painter.rect_stroke(front, 1.2 * s, Stroke::new(1.6 * s, colour), egui::StrokeKind::Middle);
                    let edge = Stroke::new(1.5 * s, colour);
                    painter.line_segment([pos2(back.left(), front.top()), pos2(back.left(), back.top())], edge);
                    painter.line_segment([back.left_top(), back.right_top()], edge);
                    painter.line_segment([back.right_top(), back.right_bottom()], edge);
                    painter.line_segment([back.right_bottom(), pos2(front.right(), back.bottom())], edge);
                }
                _ => {
                    let s = 20.0 / 24.0;
                    let o = c - vec2(12.0, 12.0) * s;
                    capped_line(&painter, o + vec2(6.4, 6.4) * s, o + vec2(17.6, 17.6) * s, 2.0 * s, colour);
                    capped_line(&painter, o + vec2(17.6, 6.4) * s, o + vec2(6.4, 17.6) * s, 2.0 * s, colour);
                }
            }
        }
        if resp.clicked() {
            match kind {
                "min" => cx.ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                "max" => cx.ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                _ => cx.ctx.send_viewport_cmd(ViewportCommand::Close),
            }
        }
    }

    // The open menu's dropdown.
    if let Some(open) = app.menu_open {
        let button = button_rects[open];
        let items = &entries[open].1;
        let mut width: f32 = 242.0;
        for item in items {
            if let Item::Entry { label, shortcut, .. } = item {
                let w = text_width(&cx.ctx, label, font(13.0, Weight::Regular))
                    + if shortcut.is_empty() { 0.0 } else { 24.0 + text_width(&cx.ctx, shortcut, font(11.0, Weight::Regular)) };
                width = width.max(w + 20.0 + 8.0 + 2.0);
            }
        }
        let height = 2.0 + 8.0 + items.iter().map(|i| if matches!(i, Item::Separator) { 9.0 } else { 30.0 }).sum::<f32>();
        let menu = Rect::from_min_size(pos2(button.left(), button.bottom() + 6.0), vec2(width, height));
        let t = if app.menu_switching { 1.0 } else { cx.anim.value(Id::new("menu-in"), 1.0, 0.15, EASE_OUT_EMPH) };
        let scale = 0.97 + 0.03 * t;
        let shown = Rect::from_center_size(menu.center() + vec2(0.0, -4.0 * (1.0 - t)), menu.size() * scale);
        let saved = cx.opacity;
        cx.opacity *= t;
        let painter = cx.painter(ui, shown);
        let mark = cx.fx.mark();
        cx.fx.shadow(shown, 10.0, &crate::gpu::chrome::shadow(0.0, 8.0, 24.0, 0.0, black(0.55)));
        cx.fx.fill(shown, [10.0; 4], cx.pal.bg5);
        cx.fx.fill(shown.shrink(1.0), [9.0; 4], cx.pal.bg3);
        cx.fx.flush(&painter, mark);
        let blocker = ui.interact(shown, Id::new("menu-block"), Sense::click());
        let mut y = shown.top() + (1.0 + 4.0) * scale;
        for (k, item) in items.iter().enumerate() {
            match item {
                Item::Separator => {
                    let line = Rect::from_min_size(pos2(shown.left() + (1.0 + 4.0 + 8.0) * scale, y + 4.0 * scale), vec2(shown.width() - 26.0 * scale, 1.0));
                    painter.rect_filled(line, 0.0, cx.pal.bg5);
                    y += 9.0 * scale;
                }
                Item::Entry { label, shortcut, action: a } => {
                    let r = Rect::from_min_size(pos2(shown.left() + 5.0 * scale, y), vec2(shown.width() - 10.0 * scale, 30.0 * scale));
                    let id = Id::new(("menu-item", open, k));
                    let resp = ui.interact(r, id, Sense::click());
                    let lit = cx.anim.flag(id.with("lit"), resp.hovered(), 0.12, EASE);
                    if lit > 0.0 {
                        let mark = cx.fx.mark();
                        cx.fx.fill(r, [8.0; 4], fade(cx.pal.bg5, lit));
                        cx.fx.flush(&painter, mark);
                    }
                    let ink = mix(cx.pal.color2, cx.pal.color1, lit);
                    painter.text(pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, font(13.0 * scale, Weight::Regular), ink);
                    if !shortcut.is_empty() {
                        painter.text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, shortcut, font(11.0 * scale, Weight::Regular), fade(ink, 0.5));
                    }
                    if resp.clicked() {
                        action = *a;
                        app.menu_open = None;
                        app.menu_switching = false;
                    }
                    y += 30.0 * scale;
                }
            }
        }
        cx.opacity = saved;
        // A press anywhere but the bar's buttons and the dropdown closes it.
        let pressed = cx.ctx.input(|i| i.pointer.any_pressed());
        let pos = cx.ctx.input(|i| i.pointer.interact_pos());
        if pressed && !blocker.hovered() && pos.is_some_and(|p| !button_rects.iter().any(|b| b.contains(p)) && !shown.contains(p)) {
            app.menu_open = None;
            app.menu_switching = false;
        }
    }

    let _ = (CursorIcon::Default, Placed::new(Pos2::ZERO, 1.0));
    action
}
