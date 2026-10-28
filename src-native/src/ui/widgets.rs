// src-native/src/ui/widgets.rs
//
// The interface's controls, each a port of its Svelte component: the glass
// button and toggle, the value sliders and their precision readouts, and the
// pills the export menu is built from.

use egui::{pos2, vec2, Align2, Color32, CursorIcon, FontId, Id, Pos2, Rect, Response, Sense, Ui};

use super::anim::{EASE, EASE_OUT_EMPH, SPRING};
use super::cx::{text_width, Cx};
use super::theme::{black, fade, font, hex, mix, rgba, white, Weight};
use crate::gpu::chrome::{shadow, GlassStyle};

/// Where a glyph's baseline sits in a galley, to align text the way CSS
/// aligns baselines.
pub fn text_at_baseline(painter: &egui::Painter, pos: Pos2, align: egui::Align, text: &str, font_id: FontId, color: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_string(), font_id, color);
    let baseline = galley.rows.first().and_then(|r| r.glyphs.first()).map_or(galley.size().y * 0.8, |g| g.pos.y);
    let x = match align {
        egui::Align::Min => pos.x,
        egui::Align::Center => pos.x - galley.size().x / 2.0,
        egui::Align::Max => pos.x - galley.size().x,
    };
    let top = pos2(x, pos.y - baseline);
    let rect = Rect::from_min_size(top, galley.size());
    painter.galley(top, galley, color);
    rect
}

pub fn baseline_of(ctx: &egui::Context, font_id: FontId) -> f32 {
    ctx.fonts(|f| {
        let g = f.layout_no_wrap("Ag".into(), font_id, Color32::WHITE);
        g.rows.first().and_then(|r| r.glyphs.first()).map_or(g.size().y * 0.8, |gl| gl.pos.y)
    })
}

fn scaled(r: Rect, s: f32) -> Rect {
    Rect::from_center_size(r.center(), r.size() * s)
}

// ── Glass button ──────────────────────────────────────────────────────────────

/// GlassToggle's thumb on its own: a square box whose glass face fades in
/// behind the icon on hover, and the whole button dips on press.
#[allow(clippy::too_many_arguments)]
pub fn glass_button(
    cx: &mut Cx,
    ui: &Ui,
    id: Id,
    rect: Rect,
    active: bool,
    disabled: bool,
    icon: impl FnOnce(&egui::Painter, Rect, f32),
) -> Response {
    let sense = if disabled { Sense::hover() } else { cx.sense(Sense::click()) };
    let resp = ui.interact(rect, id, sense);
    let pressed = resp.is_pointer_button_down_on();
    let scale = cx.anim.value(id.with("press"), if pressed { 0.92 } else { 1.0 }, 0.12, EASE);
    let face = cx.anim.flag(id.with("face"), resp.hovered() || pressed || active, 0.15, EASE);
    let saved = cx.opacity;
    if disabled {
        cx.opacity *= 0.3;
    }
    let r = scaled(rect, scale);
    let painter = cx.painter(ui, rect);
    if face > 0.0 {
        let mark = cx.fx.mark();
        let o = cx.fx.opacity;
        cx.fx.opacity = o * face;
        let face_rect = Rect::from_min_max(r.min + vec2(2.0, 3.0) * scale, r.max - vec2(2.0, 3.0) * scale);
        chip(cx, face_rect, 7.0 * scale);
        cx.fx.opacity = o;
        cx.fx.flush(&painter, mark);
    }
    icon(&painter, r, scale);
    cx.opacity = saved;
    if resp.hovered() && !disabled && cx.enabled {
        cx.ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// The chip material shared by the toggle thumb and the button face.
pub fn chip(cx: &mut Cx, r: Rect, radius: f32) {
    cx.fx.shadow(r, radius, &shadow(1.5, 1.8, 5.0, 0.0, black(0.55)));
    cx.fx.glass(r, [radius; 4], &GlassStyle::chip());
    cx.fx.inset(r, radius, &shadow(-4.0, -4.0, 6.0, -4.0, rgba(12, 26, 40, 0.25)));
}

// ── Glass toggle ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct Stop {
    pub x: f32,
    pub w: f32,
}

/// A flat black well with a glass thumb inset inside it. The icons are drawn
/// by the caller over the thumb, against the track's top-left corner.
#[allow(clippy::too_many_arguments)]
pub fn glass_toggle(
    cx: &mut Cx,
    ui: &Ui,
    id: Id,
    origin: Pos2,
    width: f32,
    checked: bool,
    off: Stop,
    on: Stop,
    icons: impl FnOnce(&egui::Painter, Pos2, f32),
) -> Response {
    let rect = Rect::from_min_size(origin, vec2(width, 28.0));
    let resp = ui.interact(rect, id, cx.sense(Sense::click()));
    let pressed = resp.is_pointer_button_down_on();
    let scale = cx.anim.value(id.with("press"), if pressed { 0.97 } else { 1.0 }, 0.12, EASE);
    let stop = if checked { on } else { off };
    let x = cx.anim.value(id.with("x"), stop.x, 0.2, EASE_OUT_EMPH);
    let w = cx.anim.value(id.with("w"), stop.w, 0.2, EASE_OUT_EMPH);
    let grow = cx.anim.value(id.with("hover"), if resp.hovered() && cx.enabled { 1.08 } else { 1.0 }, 0.12, EASE);

    let painter = cx.painter(ui, rect);
    let track = scaled(rect, scale);
    let at = |p: Pos2| track.min + (p.to_vec2()) * scale;
    let mark = cx.fx.mark();
    cx.fx.fill(track, [10.0 * scale; 4], cx.pal.bg1);
    cx.fx.inset(track, 10.0 * scale, &shadow(0.0, 1.0, 2.0, 0.0, black(0.9)));
    cx.fx.ring(track, [10.0 * scale; 4], 1.0, white(0.04));
    let thumb = Rect::from_min_size(at(pos2(x, 3.0)), vec2(w, 22.0) * scale);
    let thumb = scaled(thumb, grow);
    chip(cx, thumb, 7.0 * scale * grow);
    cx.fx.flush(&painter, mark);
    icons(&painter, track.min, scale);
    if resp.hovered() && cx.enabled {
        cx.ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

// ── Slider track ──────────────────────────────────────────────────────────────

/// Interpolate two hex colours, as the slider's middle stop is.
pub fn lerp_hex(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// The pointer's size, from its rest, hover and drag states.
fn pointer_size(cx: &mut Cx, id: Id, hovered: bool, dragging: bool) -> egui::Vec2 {
    let (w, h) = if dragging {
        (12.0, 22.0)
    } else if hovered {
        (5.0, 18.0)
    } else {
        (4.0, 16.0)
    };
    vec2(
        cx.anim.value(id.with("pw"), w, 0.2, EASE_OUT_EMPH),
        cx.anim.value(id.with("ph"), h, 0.2, EASE_OUT_EMPH),
    )
}

/// The slider container's drop shadows, under one of its pieces.
fn lifted(cx: &mut Cx, r: Rect, radius: f32) {
    cx.fx.shadow(r, radius, &shadow(0.0, 1.0, 1.0, 0.0, black(0.3)));
    cx.fx.shadow(r, radius, &shadow(0.0, 0.0, 12.0, 0.0, black(0.25)));
}

fn pointer(cx: &mut Cx, center: Pos2, size: egui::Vec2) {
    let r = Rect::from_center_size(center, size);
    lifted(cx, r, 5.0);
    cx.fx.shadow(r, 5.0, &shadow(0.0, 2.0, 8.0, 0.0, black(0.15)));
    cx.fx.fill(r, [5.0f32.min(size.x / 2.0); 4], cx.pal.color1);
}

fn bar(cx: &mut Cx, x0: f32, x1: f32, y: f32, from: Color32, to: Color32) {
    if x1 - x0 <= 0.0 {
        return;
    }
    let r = Rect::from_min_max(pos2(x0, y - 2.0), pos2(x1, y + 2.0));
    lifted(cx, r, 1.0);
    if from == to {
        cx.fx.fill(r, [1.0; 4], from);
    } else {
        cx.fx.linear(r, [1.0; 4], pos2(x0, y), pos2(x1, y), from, to);
    }
}

/// Where a pointer at `pct` sits in a track `rect` wide.
fn pointer_x(rect: Rect, pct: f32) -> f32 {
    rect.left() + pct * (rect.width() - 4.0) + 2.0
}

pub struct SliderLook {
    pub start: Color32,
    pub end: Color32,
    /// Both bars in one colour, as the export menu's are.
    pub flat: Option<Color32>,
}

/// One gradient track and its pointer, in `rect` (the 22px slider box).
pub fn draw_single(cx: &mut Cx, ui: &Ui, id: Id, rect: Rect, pct: f32, pointer_pct: f32, look: &SliderLook, hovered: bool, dragging: bool) {
    let painter = cx.painter(ui, rect);
    let mark = cx.fx.mark();
    let y = rect.center().y;
    let mid = lerp_hex(look.start, look.end, pct);
    let at = rect.left() + rect.width() * pct;
    let (a0, a1, b0, b1) = match look.flat {
        Some(c) => (c, c, c, c),
        None => (look.start, mid, mid, look.end),
    };
    bar(cx, rect.left(), at - 12.0, y, a0, a1);
    bar(cx, at + 12.0, rect.right(), y, b0, b1);
    let size = pointer_size(cx, id, hovered, dragging);
    pointer(cx, pos2(pointer_x(rect, pointer_pct), y), size);
    cx.fx.flush(&painter, mark);
}

// ── Value scales ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum Scale {
    Linear,
    /// Spaced by 1/value, for a quantity whose effect scales with the reciprocal.
    Reciprocal,
}

#[derive(Clone, Copy)]
pub struct Track {
    pub min: f64,
    pub max: f64,
    pub scale: Scale,
    pub center: Option<f64>,
}

impl Track {
    fn warp(&self, v: f64) -> f64 {
        if self.scale == Scale::Reciprocal {
            1.0 / v
        } else {
            v
        }
    }

    fn center(&self) -> f64 {
        self.center.unwrap_or((self.min + self.max) / 2.0)
    }

    /// Each half is mapped independently so the centre lands on the midpoint.
    pub fn fraction(&self, v: f64) -> f64 {
        let wc = self.warp(self.center());
        if v <= self.center() {
            let w0 = self.warp(self.min);
            if w0 == wc {
                0.5
            } else {
                0.5 * ((self.warp(v) - w0) / (wc - w0))
            }
        } else {
            let w1 = self.warp(self.max);
            if w1 == wc {
                0.5
            } else {
                0.5 + 0.5 * ((self.warp(v) - wc) / (w1 - wc))
            }
        }
    }

    pub fn value(&self, f: f64) -> f64 {
        let wc = self.warp(self.center());
        let w = if f <= 0.5 {
            self.warp(self.min) + (wc - self.warp(self.min)) * (f / 0.5)
        } else {
            wc + (self.warp(self.max) - wc) * ((f - 0.5) / 0.5)
        };
        if self.scale == Scale::Reciprocal {
            1.0 / w
        } else {
            w
        }
    }
}

/// What a control did this frame.
#[derive(Default, Clone, Copy, PartialEq)]
pub struct Interaction {
    /// The user began touching the value: snapshot it for history.
    pub started: bool,
    /// The user let go of it: commit it.
    pub ended: bool,
}

impl Interaction {
    pub fn merge(&mut self, o: Interaction) {
        self.started |= o.started;
        self.ended |= o.ended;
    }
}

#[derive(Clone, Copy)]
struct Drag {
    near: bool,
    origin_x: f32,
    origin_fraction: f64,
}

/// SingleValSlider: a press near the pointer drags it relatively, anywhere else
/// jumps it there first. A double click puts the default back.
#[allow(clippy::too_many_arguments)]
pub fn single_slider(cx: &mut Cx, ui: &Ui, id: Id, rect: Rect, value: &mut f64, default: f64, track: Track, look: &SliderLook) -> Interaction {
    let mut out = Interaction::default();
    let resp = ui.interact(rect, id, cx.sense(Sense::click_and_drag()));
    let pct = track.fraction(*value).clamp(0.0, 1.0);
    let ptr = ui.ctx().input(|i| i.pointer.interact_pos());
    let drag_id = id.with("drag");
    if resp.drag_started() || (resp.is_pointer_button_down_on() && cx.ctx.data(|d| d.get_temp::<Drag>(drag_id)).is_none()) {
        if let Some(p) = ptr {
            let center = rect.left() + pct as f32 * rect.width();
            let near = (p.x - center).abs() <= 20.0;
            cx.ctx.data_mut(|d| d.insert_temp(drag_id, Drag { near, origin_x: p.x, origin_fraction: track.fraction(*value) }));
            out.started = true;
            if !near {
                *value = track.value(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64).clamp(track.min, track.max);
            }
        }
    }
    let drag = cx.ctx.data(|d| d.get_temp::<Drag>(drag_id));
    if let (Some(d), Some(p)) = (drag, ptr) {
        if resp.dragged() {
            let f = if d.near {
                d.origin_fraction + ((p.x - d.origin_x) / rect.width()) as f64
            } else {
                ((p.x - rect.left()) / rect.width()) as f64
            };
            *value = track.value(f.clamp(0.0, 1.0)).clamp(track.min, track.max);
        }
    }
    if drag.is_some() && !resp.is_pointer_button_down_on() && !resp.dragged() {
        cx.ctx.data_mut(|d| d.remove::<Drag>(drag_id));
        out.ended = true;
    }
    if resp.double_clicked() {
        *value = default;
        out.started = true;
        out.ended = true;
    }
    let pct = track.fraction(*value).clamp(0.0, 1.0) as f32;
    let hover_ptr = ptr.is_some_and(|p| (p - pos2(pointer_x(rect, pct), rect.center().y)).abs().max_elem() <= 10.0) && resp.hovered();
    draw_single(cx, ui, id, rect, pct, pct, look, hover_ptr, drag.is_some() || out.started);
    out
}

// ── Precision value ───────────────────────────────────────────────────────────

/// The text field a precision value turns into on click.
#[derive(Default)]
pub struct Editing {
    pub id: Option<Id>,
    pub text: String,
    pub focus: bool,
}

#[derive(Clone, Copy)]
pub struct Precision<'a> {
    pub default: f64,
    pub min: f64,
    pub max: f64,
    pub unit: &'a str,
    pub step: f64,
    pub keyboard_step: f64,
    pub decimals: usize,
}

impl Precision<'_> {
    pub fn format(&self, v: f64) -> String {
        format!("{}{}", js_to_fixed(v, self.decimals), self.unit)
    }

    fn clamp(&self, v: f64) -> f64 {
        v.max(self.min).min(self.max)
    }
}

/// `toFixed`, which never prints a negative zero.
pub fn js_to_fixed(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v);
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

pub const VALUE_FONT: f32 = 14.0;
pub const VALUE_PAD: f32 = 5.6;

/// The size the readout takes for `text`.
pub fn precision_size(ctx: &egui::Context, text: &str) -> egui::Vec2 {
    vec2(text_width(ctx, text, font(VALUE_FONT, Weight::Medium)) + VALUE_PAD * 2.0, VALUE_FONT + VALUE_PAD * 2.0)
}

/// PrecisionVal: drag sideways to scrub with the cursor hidden and wrapping at
/// the window's edges, click to type, Backspace over it to reset.
pub fn precision(cx: &mut Cx, ui: &mut Ui, id: Id, rect: Rect, value: &mut f64, p: Precision<'_>, editing: &mut Editing) -> Interaction {
    let mut out = Interaction::default();
    let is_editing = editing.id == Some(id);
    let resp = ui.interact(rect, id, cx.sense(Sense::click_and_drag()));
    let hover = cx.anim.flag(id.with("bg"), (resp.hovered() && cx.enabled) || is_editing, 0.2, EASE_OUT_EMPH);
    let painter = cx.painter(ui, rect);
    if hover > 0.0 {
        let mark = cx.fx.mark();
        cx.fx.fill(rect, [4.0; 4], fade(cx.pal.bg2, hover));
        cx.fx.flush(&painter, mark);
    }

    if is_editing {
        let mut text = std::mem::take(&mut editing.text);
        let te = egui::TextEdit::singleline(&mut text)
            .font(font(VALUE_FONT, Weight::Medium))
            .text_color(cx.pal.color1s)
            .horizontal_align(egui::Align::Max)
            .frame(false)
            .margin(egui::Margin::symmetric(VALUE_PAD as i8, 5))
            .desired_width(rect.width());
        let r = ui.put(rect, te);
        if editing.focus {
            r.request_focus();
            // Select the number, not the unit.
            let end = text.chars().count().saturating_sub(p.unit.chars().count());
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(end))));
                state.store(ui.ctx(), r.id);
            }
            editing.focus = false;
        }
        let (up, down, enter) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape),
            )
        });
        if up || down {
            *value = p.clamp(*value + if up { p.keyboard_step } else { -p.keyboard_step });
            text = p.format(*value);
            editing.focus = true;
        }
        if enter || r.lost_focus() || (!r.has_focus() && !editing.focus && ui.input(|i| i.pointer.any_pressed()) && !r.hovered()) {
            let stripped = text.trim().trim_end_matches(p.unit).trim();
            if let Some(v) = parse_float(stripped) {
                *value = p.clamp(v);
            }
            editing.id = None;
            out.ended = true;
        } else {
            editing.text = text;
        }
        return out;
    }

    let shown = p.format(*value);
    painter.text(
        pos2(rect.right() - VALUE_PAD, rect.center().y),
        Align2::RIGHT_CENTER,
        &shown,
        font(VALUE_FONT, Weight::Medium),
        cx.pal.color1s,
    );

    if resp.hovered() && cx.enabled {
        cx.ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    let dragging_id = id.with("scrub");
    if resp.is_pointer_button_down_on() && resp.drag_started() {
        out.started = true;
        cx.ctx.data_mut(|d| d.insert_temp(dragging_id, true));
    }
    if resp.dragged() {
        let dx = ui.input(|i| i.pointer.delta().x);
        if dx.abs() <= 100.0 {
            *value = p.clamp(*value + dx as f64 * p.step);
        }
        cx.ctx.set_cursor_icon(CursorIcon::None);
        wrap_cursor(&cx.ctx);
    }
    if resp.drag_stopped() {
        cx.ctx.data_mut(|d| d.remove::<bool>(dragging_id));
        out.ended = true;
    }
    if resp.clicked() {
        out.started = true;
        editing.id = Some(id);
        editing.text = shown;
        editing.focus = true;
    }
    if resp.hovered() && cx.enabled && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)) {
        out.started = true;
        *value = p.default;
        out.ended = true;
    }
    out
}

/// parseFloat: the longest leading number, ignoring what follows.
pub fn parse_float(s: &str) -> Option<f64> {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    let mut seen_digit = false;
    let mut seen_dot = false;
    if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
        end += 1;
    }
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        end += 1;
    }
    if !seen_digit {
        return None;
    }
    s[..end].parse().ok()
}

/// Moves the cursor to the far side when a scrub reaches a window edge, so a
/// drag can run on for as long as the hand does.
fn wrap_cursor(ctx: &egui::Context) {
    let Some(p) = ctx.input(|i| i.pointer.latest_pos()) else { return };
    let screen = ctx.screen_rect();
    let pad = 2.0;
    let mut next = p;
    if p.x <= screen.left() + pad {
        next.x = screen.right() - pad - 1.0;
    } else if p.x >= screen.right() - pad {
        next.x = screen.left() + pad + 1.0;
    }
    if p.y <= screen.top() + pad {
        next.y = screen.bottom() - pad - 1.0;
    } else if p.y >= screen.bottom() - pad {
        next.y = screen.top() + pad + 1.0;
    }
    if next != p {
        ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(next));
    }
}

// ── Slider groups ─────────────────────────────────────────────────────────────

pub struct SingleSpec<'a> {
    pub name: &'a str,
    pub unit: &'a str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
    /// Let the typed and dragged value run past the rails, to these.
    pub hard: Option<(f64, f64)>,
    pub decimals: usize,
    pub drag_step: f64,
    pub keyboard_step: f64,
    pub scale: Scale,
    pub center: Option<f64>,
    pub start: Color32,
    pub end: Color32,
}

impl<'a> SingleSpec<'a> {
    pub fn new(name: &'a str, unit: &'a str, default: f64, min: f64, max: f64) -> Self {
        Self {
            name,
            unit,
            default,
            min,
            max,
            hard: None,
            decimals: 2,
            drag_step: 0.01,
            keyboard_step: 0.1,
            scale: Scale::Linear,
            center: None,
            start: hex(0x4d4d4d),
            end: hex(0xffffff),
        }
    }

    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }

    pub fn steps(mut self, drag: f64, keyboard: f64) -> Self {
        self.drag_step = drag;
        self.keyboard_step = keyboard;
        self
    }

    pub fn colours(mut self, start: u32, end: u32) -> Self {
        self.start = hex(start);
        self.end = hex(end);
        self
    }

    pub fn colour_end(mut self, end: u32) -> Self {
        self.end = hex(end);
        self
    }
}

/// Height of a single-value group: the label row pulled up 3px and its value
/// overlapping the 22px track by 6.
pub const SINGLE_HEIGHT: f32 = -3.0 + VALUE_FONT + VALUE_PAD * 2.0 - 6.0 + 22.0;

/// SingleValSliderGroup at `top`, `width` wide. Returns (old, new) once a
/// change is let go of.
pub fn single_group(cx: &mut Cx, ui: &mut Ui, id: Id, x: f32, top: f32, width: f32, value: &mut f64, s: &SingleSpec<'_>, editing: &mut Editing) -> Option<(f64, f64)> {
    let snap_id = id.with("snapshot");
    let row_top = top - 3.0;
    let p = Precision {
        default: s.default,
        min: s.hard.map_or(s.min, |h| h.0),
        max: s.hard.map_or(s.max, |h| h.1),
        unit: s.unit,
        step: s.drag_step,
        keyboard_step: s.keyboard_step,
        decimals: s.decimals,
    };
    let size = precision_size(&cx.ctx, &p.format(*value));
    let value_rect = Rect::from_min_size(pos2(x + width - size.x, row_top), size);
    let baseline = row_top + VALUE_PAD + baseline_of(&cx.ctx, font(VALUE_FONT, Weight::Medium));
    {
        let r = Rect::from_min_size(pos2(x, row_top), vec2(width, size.y));
        let painter = cx.painter(ui, r);
        text_at_baseline(&painter, pos2(x, baseline + 1.0), egui::Align::Min, s.name, font(12.0, Weight::Regular), fade(Color32::BLACK, 0.35));
        text_at_baseline(&painter, pos2(x, baseline), egui::Align::Min, s.name, font(12.0, Weight::Regular), cx.pal.color2);
    }
    let mut interaction = precision(cx, ui, id.with("value"), value_rect, value, p, editing);
    let slider_rect = Rect::from_min_size(pos2(x, row_top + size.y - 6.0), vec2(width - 6.0, 22.0));
    let track = Track { min: s.min, max: s.max, scale: s.scale, center: s.center };
    let look = SliderLook { start: s.start, end: s.end, flat: None };
    interaction.merge(single_slider(cx, ui, id.with("slider"), slider_rect, value, s.default, track, &look));
    commit(cx, snap_id, *value, interaction)
}

/// Snapshot on interaction start, report (old, new) on its end.
pub fn commit(cx: &mut Cx, snap_id: Id, value: f64, i: Interaction) -> Option<(f64, f64)> {
    if i.started && cx.ctx.data(|d| d.get_temp::<f64>(snap_id)).is_none() {
        cx.ctx.data_mut(|d| d.insert_temp(snap_id, value));
    }
    if i.ended {
        let old = cx.ctx.data_mut(|d| d.remove_temp::<f64>(snap_id));
        if let Some(old) = old {
            if old != value {
                return Some((old, value));
            }
        }
    }
    None
}

/// A pointer on a multi-pointer track, hit-tested over its 20px box.
#[allow(clippy::too_many_arguments)]
fn multi_pointer(cx: &mut Cx, ui: &Ui, id: Id, track: Rect, pct: f32, sense: bool) -> (Response, bool) {
    let center = pos2(pointer_x(track, pct), track.center().y);
    let hit = Rect::from_center_size(center, vec2(20.0, 22.0));
    let resp = ui.interact(hit, id, if sense { cx.sense(Sense::click_and_drag()) } else { Sense::hover() });
    let dragging = resp.dragged() || resp.is_pointer_button_down_on();
    (resp, dragging)
}

/// TripleValSlider (with `b` set) or DoubleValSlider: black, gamma and white.
/// Returns which pointer changed, as (index, interaction).
#[allow(clippy::too_many_arguments)]
pub fn multi_slider(
    cx: &mut Cx,
    ui: &Ui,
    id: Id,
    rect: Rect,
    a: &mut f64,
    b: Option<(&mut f64, f64, f64, f64)>,
    c: &mut f64,
    defaults: (f64, f64),
    min: f64,
    max: f64,
    inner: Color32,
    outer: Color32,
) -> [Interaction; 3] {
    let mut out = [Interaction::default(); 3];
    let pct = |v: f64| ((v - min) / (max - min)) as f32;
    let (pa, pc) = (pct(*a), pct(*c));
    let mut b = b;
    let pb = b.as_ref().map(|(v, lo, hi, _)| pa + ((**v - *lo) / (*hi - *lo)) as f32 * (pc - pa));

    // C last, so it sits on top where the pointers overlap.
    let order: Vec<usize> = if b.is_some() { vec![0, 1, 2] } else { vec![0, 2] };
    let mut drags = [false; 3];
    let ptr = ui.ctx().input(|i| i.pointer.interact_pos());
    for &k in &order {
        let p = match k {
            0 => pa,
            1 => pb.unwrap(),
            _ => pc,
        };
        let pid = id.with(k);
        let (resp, dragging) = multi_pointer(cx, ui, pid, rect, p, true);
        drags[k] = dragging;
        let drag_id = pid.with("drag");
        if (resp.drag_started() || resp.is_pointer_button_down_on()) && cx.ctx.data(|d| d.get_temp::<(f32, f64)>(drag_id)).is_none() {
            if let Some(pp) = ptr {
                let initial = match k {
                    0 => *a,
                    1 => *b.as_ref().unwrap().0,
                    _ => *c,
                };
                cx.ctx.data_mut(|d| d.insert_temp(drag_id, (pp.x, initial)));
                out[k].started = true;
            }
        }
        if let (Some((x0, initial)), Some(pp)) = (cx.ctx.data(|d| d.get_temp::<(f32, f64)>(drag_id)), ptr) {
            if resp.dragged() {
                let delta = (pp.x - x0) / rect.width();
                match k {
                    0 => {
                        *a = (initial + delta as f64 * (max - min)).clamp(min, max);
                        if b.is_none() && *a > *c {
                            *c = *a;
                        }
                    }
                    1 => {
                        let span = (pc - pa) as f64 * 100.0;
                        if span >= 0.001 {
                            let (v, lo, hi, _) = b.as_mut().unwrap();
                            **v = (initial + delta as f64 * (100.0 / span) * (*hi - *lo)).clamp(*lo, *hi);
                        }
                    }
                    _ => {
                        *c = (initial + delta as f64 * (max - min)).clamp(min, max);
                        if b.is_none() && *c < *a {
                            *a = *c;
                        }
                    }
                }
            }
            if !resp.dragged() && !resp.is_pointer_button_down_on() {
                cx.ctx.data_mut(|d| d.remove::<(f32, f64)>(drag_id));
                out[k].ended = true;
            }
        }
        if resp.double_clicked() {
            match k {
                0 => *a = defaults.0,
                1 => {
                    let (v, _, _, d) = b.as_mut().unwrap();
                    **v = *d;
                }
                _ => *c = defaults.1,
            }
            out[k].started = true;
            out[k].ended = true;
        }
    }

    let (pa, pc) = (pct(*a), pct(*c));
    let pb = b.as_ref().map(|(v, lo, hi, _)| pa + ((**v - *lo) / (*hi - *lo)) as f32 * (pc - pa));
    let painter = cx.painter(ui, rect);
    let mark = cx.fx.mark();
    let y = rect.center().y;
    let w = rect.width();
    let x = |p: f32| rect.left() + p * w;
    bar(cx, rect.left(), x(pa) - 12.0, y, outer, outer);
    match pb {
        Some(pb) => {
            bar(cx, x(pa) + 12.0, x(pb) - 12.0, y, inner, inner);
            bar(cx, x(pb) + 12.0, x(pc) - 12.0, y, inner, inner);
        }
        None => bar(cx, x(pa) + 12.0, x(pc) - 12.0, y, inner, inner),
    }
    bar(cx, x(pc) + 12.0, rect.right(), y, outer, outer);
    for &k in &order {
        let p = match k {
            0 => pa,
            1 => pb.unwrap(),
            _ => pc,
        };
        let center = pos2(pointer_x(rect, p), y);
        let hovered = ptr.is_some_and(|pp| Rect::from_center_size(center, vec2(20.0, 20.0)).contains(pp)) && cx.enabled;
        let size = pointer_size(cx, id.with(k), hovered, drags[k]);
        pointer(cx, center, size);
    }
    cx.fx.flush(&painter, mark);
    out
}

// ── Int slider ────────────────────────────────────────────────────────────────

const MAGNET_STRENGTH: f64 = 0.72;

/// SingleValSliderInt: snapped to whole steps, its pointer springing between
/// them, or following the cursor linearly when `springy` is off.
#[allow(clippy::too_many_arguments)]
pub fn int_slider(cx: &mut Cx, ui: &Ui, id: Id, rect: Rect, value: &mut u8, default: u8, min: f64, max: f64, springy: bool) {
    let resp = ui.interact(rect, id, cx.sense(Sense::click_and_drag()));
    let raw_id = id.with("raw");
    let drag_id = id.with("drag");
    let ptr = ui.ctx().input(|i| i.pointer.interact_pos());
    let pct_of = |v: f64| ((v - min) / (max - min)) as f32;
    let snap = |raw: f64| ((raw - min).round() + min).clamp(min, max);
    let mut raw = cx.ctx.data(|d| d.get_temp::<f64>(raw_id)).unwrap_or(*value as f64);
    let dragging = cx.ctx.data(|d| d.get_temp::<(f32, f64, bool)>(drag_id));
    if !resp.is_pointer_button_down_on() && dragging.is_none() {
        raw = *value as f64;
    }
    if resp.is_pointer_button_down_on() && dragging.is_none() {
        if let Some(p) = ptr {
            let center = rect.left() + pct_of(*value as f64) * rect.width();
            let near = (p.x - center).abs() <= 20.0;
            cx.ctx.data_mut(|d| d.insert_temp(drag_id, (p.x, raw, near)));
            if !near {
                raw = (((p.x - rect.left()) / rect.width()) as f64 * (max - min) + min).clamp(min, max);
            }
        }
    }
    if let (Some((x0, raw0, near)), Some(p)) = (cx.ctx.data(|d| d.get_temp::<(f32, f64, bool)>(drag_id)), ptr) {
        if resp.dragged() {
            raw = if near {
                (raw0 + ((p.x - x0) / rect.width()) as f64 * (max - min)).clamp(min, max)
            } else {
                (((p.x - rect.left()) / rect.width()) as f64 * (max - min) + min).clamp(min, max)
            };
        }
        if !resp.dragged() && !resp.is_pointer_button_down_on() {
            cx.ctx.data_mut(|d| d.remove::<(f32, f64, bool)>(drag_id));
            raw = snap(raw);
        }
    }
    *value = snap(raw) as u8;
    if resp.double_clicked() {
        *value = default;
        raw = default as f64;
    }
    cx.ctx.data_mut(|d| d.insert_temp(raw_id, raw));

    let bar_pct = pct_of(*value as f64);
    let pointer_pct = if springy {
        cx.anim.value(id.with("spring"), bar_pct, 0.12, SPRING)
    } else {
        // Magnetic display only applies when springy; otherwise linear.
        let _ = MAGNET_STRENGTH;
        pct_of(raw)
    };
    let look = SliderLook { start: cx.pal.slider_track, end: cx.pal.slider_track, flat: Some(cx.pal.slider_track) };
    let hovered = resp.hovered() && ptr.is_some_and(|p| (p.x - pointer_x(rect, pointer_pct)).abs() <= 10.0);
    draw_single(cx, ui, id, rect, bar_pct, pointer_pct, &look, hovered, cx.ctx.data(|d| d.get_temp::<(f32, f64, bool)>(drag_id)).is_some());
}

// ── Pills ─────────────────────────────────────────────────────────────────────

/// The export menu's buttons: a quiet fill, or the ink colour when selected.
pub fn pill(cx: &mut Cx, ui: &Ui, id: Id, rect: Rect, label: &str, active: bool) -> Response {
    let resp = ui.interact(rect, id, cx.sense(Sense::click()));
    let hover = cx.anim.flag(id.with("hover"), resp.hovered(), 0.2, EASE_OUT_EMPH);
    let on = cx.anim.flag(id.with("on"), active, 0.2, EASE_OUT_EMPH);
    let painter = cx.painter(ui, rect);
    let bg = mix(mix(cx.pal.bg4, cx.pal.bg5, hover), cx.pal.color1, on);
    let fg = mix(cx.pal.color2, cx.pal.bg1, on);
    let mark = cx.fx.mark();
    cx.fx.fill(rect, [6.0; 4], bg);
    cx.fx.flush(&painter, mark);
    let weight = if active { Weight::SemiBold } else { Weight::Regular };
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font(13.0, weight), fg);
    if resp.hovered() && cx.enabled {
        cx.ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}
