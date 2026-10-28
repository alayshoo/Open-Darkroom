// src-native/src/ui/cx.rs
//
// What every widget draws with: the frame's chrome primitives, the tweens, the
// blended palette, and the group opacity and edge fades the CSS would apply
// to whatever the widget sits inside.

use egui::{Align2, Color32, FontId, Pos2, Rect, Ui};

use super::anim::Anims;
use super::theme::{fade, Palette};
use crate::gpu::chrome::Fx;

/// A scrolling region whose content fades out towards its edges, as the
/// side bar's pages are masked.
#[derive(Clone, Copy)]
pub struct Mask {
    pub rect: Rect,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Mask {
    pub fn at(&self, r: Rect) -> f32 {
        let c = r.center();
        let ramp = |d: f32, len: f32| if len <= 0.0 { 1.0 } else { (d / len).clamp(0.0, 1.0) };
        ramp(c.y - self.rect.top(), self.top)
            .min(ramp(self.rect.bottom() - c.y, self.bottom))
            .min(ramp(c.x - self.rect.left(), self.left))
            .min(ramp(self.rect.right() - c.x, self.right))
    }
}

pub struct Cx<'a> {
    pub ctx: egui::Context,
    pub fx: &'a mut Fx,
    pub anim: &'a mut Anims,
    pub pal: Palette,
    /// The CSS opacity of the group being drawn.
    pub opacity: f32,
    pub masks: Vec<Mask>,
    /// Off while a group is inert: drawn, but not interactive.
    pub enabled: bool,
    pub ppp: f32,
    /// Keeps ids apart for a control drawn on more than one page.
    pub salt: usize,
}

impl Cx<'_> {
    /// The opacity a widget at `r` is drawn at, its group's and its mask's.
    pub fn alpha(&self, r: Rect) -> f32 {
        self.masks.iter().fold(self.opacity, |o, m| o * m.at(r))
    }

    /// A painter for `ui` at the opacity of something at `r`, with the chrome
    /// primitives following it.
    pub fn painter(&mut self, ui: &Ui, r: Rect) -> egui::Painter {
        let o = self.alpha(r);
        self.fx.opacity = o;
        let mut p = ui.painter().clone();
        p.set_opacity(o);
        p
    }

    pub fn sense(&self, sense: egui::Sense) -> egui::Sense {
        if self.enabled {
            sense
        } else {
            egui::Sense::hover()
        }
    }

    pub fn now(&self) -> f64 {
        self.anim.now
    }
}

/// Text with the faint drop shadow the side bar lifts its labels with.
pub fn shadow_text(painter: &egui::Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, shadow: f32) -> Rect {
    if shadow > 0.0 {
        painter.text(pos + egui::vec2(0.0, 1.0), align, text, font.clone(), fade(Color32::BLACK, shadow));
    }
    painter.text(pos, align, text, font, color)
}

pub fn text_width(ctx: &egui::Context, text: &str, font: FontId) -> f32 {
    ctx.fonts(|f| f.layout_no_wrap(text.to_string(), font, Color32::WHITE).size().x)
}
