// src-native/src/ui/frame.rs
//
// One frame: the interface built over four layers, then the GPU brought up to
// date with what it changed, then the chrome handed to egui to draw. Nothing
// is drawn unless something asked for it.

use egui::{pos2, vec2, Color32, Id, Order, Rect, Shape, Ui};

use super::anim::EASE;
use super::canvas;
use super::cx::Cx;
use super::geo::Geo;
use super::theme::{hex, mix, Palette};
use super::titlebar::MenuAction;
use crate::app::{App, Msg};
use crate::gpu::chrome::{ChromeResources, Fx};
use crate::model::viewport::view_frame;

/// A full-window Ui on its own layer. Not an Area: an Area registers a
/// widget over its whole rect, and egui hides every widget on a lower layer
/// that such a rect contains.
fn layer<R>(ctx: &egui::Context, name: &str, order: Order, body: impl FnOnce(&mut Ui) -> R) -> R {
    let screen = ctx.screen_rect();
    let id = Id::new(name);
    let mut ui = Ui::new(ctx.clone(), id, egui::UiBuilder::new().layer_id(egui::LayerId::new(order, id)).max_rect(screen));
    ui.set_clip_rect(screen);
    body(&mut ui)
}

/// Edges the undecorated window is resized from.
fn resize_edges(ctx: &egui::Context, ui: &Ui) {
    use egui::viewport::ResizeDirection as D;
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let s = ctx.screen_rect();
    let e = 4.0;
    let edges = [
        (Rect::from_min_max(s.min, pos2(s.min.x + e * 2.0, s.min.y + e * 2.0)), D::NorthWest, egui::CursorIcon::ResizeNwSe),
        (Rect::from_min_max(pos2(s.max.x - e * 2.0, s.min.y), pos2(s.max.x, s.min.y + e * 2.0)), D::NorthEast, egui::CursorIcon::ResizeNeSw),
        (Rect::from_min_max(pos2(s.min.x, s.max.y - e * 2.0), pos2(s.min.x + e * 2.0, s.max.y)), D::SouthWest, egui::CursorIcon::ResizeNeSw),
        (Rect::from_min_max(s.max - vec2(e * 2.0, e * 2.0), s.max), D::SouthEast, egui::CursorIcon::ResizeNwSe),
        (Rect::from_min_max(s.min, pos2(s.max.x, s.min.y + e)), D::North, egui::CursorIcon::ResizeVertical),
        (Rect::from_min_max(pos2(s.min.x, s.max.y - e), s.max), D::South, egui::CursorIcon::ResizeVertical),
        (Rect::from_min_max(s.min, pos2(s.min.x + e, s.max.y)), D::West, egui::CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(pos2(s.max.x - e, s.min.y), s.max), D::East, egui::CursorIcon::ResizeHorizontal),
    ];
    for (i, (r, dir, cursor)) in edges.into_iter().enumerate() {
        let resp = ui.interact(r, Id::new(("resize", i)), egui::Sense::drag());
        if resp.hovered() {
            ctx.set_cursor_icon(cursor);
        }
        if resp.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = self.now();
        self.drain_messages();
        self.handle_keys(ctx);
        self.export.menu_toggled = false;
        let tweening = self.step_tweens();
        let previous_crop = self.crop;

        let mut anims = std::mem::take(&mut self.anims);
        anims.begin_frame(now);
        let ppp = ctx.pixels_per_point();
        let screen = ctx.screen_rect();
        let mut fx = Fx::new(ppp);
        let theme = anims.value("theme", if self.is_darkroom { 1.0 } else { 0.0 }, 0.5, EASE);
        let pal = Palette::blend(theme);
        let backdrop_t = anims.value("canvas-backdrop", if self.is_darkroom { 1.0 } else { 0.0 }, 0.12, EASE);
        let backdrop = mix(hex(0x262626), hex(0x1f1f1f), backdrop_t);
        let colour_bar = self.debug_tools && self.colour_space_debug.enabled;
        let geo = Geo::new(screen, colour_bar);
        let layout = canvas::layout(self, &geo, ppp);
        canvas::settle_zoom(self, layout.as_ref());
        let layout = canvas::layout(self, &geo, ppp);

        let mut cx = Cx { ctx: ctx.clone(), fx: &mut fx, anim: &mut anims, pal, opacity: 1.0, masks: Vec::new(), enabled: true, ppp, salt: 0 };
        let mut menu_action = None;

        // The image, and what moves it, under everything.
        let (backdrop_slot, canvas_slot) = layer(ctx, "background", Order::Background, |ui| {
            let painter = ui.painter().clone();
            let slots = (painter.add(Shape::Noop), painter.add(Shape::Noop));
            canvas::preview_input(self, &mut cx, ui, layout.as_ref());
            if self.is_cropping {
                if let Some(l) = &layout {
                    canvas::crop_overlay(self, &mut cx, ui, l);
                }
            }
            slots
        });

        // The scrim, the icon column and the panels.
        layer(ctx, "chrome", Order::Middle, |ui| {
            let painter = ui.painter().clone();
            let mark = cx.fx.mark();
            cx.fx.scrim(screen);
            cx.fx.flush(&painter, mark);
            canvas::toolbar(self, &mut cx, ui, &geo);
            if let Some(bar) = geo.colour_bar {
                self.ensure_colour_space();
                super::debug::colour_space_panel(self, &mut cx, ui, bar);
            }
            if self.debug_tools && self.debug.stats {
                super::debug::stats_overlay(self, &mut cx, ui, geo.slot);
            }
            super::sidebar::sidebar(self, &mut cx, ui, &geo);
            super::overlays::export_menu(self, &mut cx, ui, geo.footer);
        });

        layer(ctx, "modal", Order::Foreground, |ui| {
            super::overlays::settings_modal(self, &mut cx, ui);
        });

        // The title bar sits over the settings scrim; the export modal over all.
        layer(ctx, "titlebar", Order::Tooltip, |ui| {
            menu_action = super::titlebar::titlebar(self, &mut cx, ui);
            super::overlays::export_modal(self, &mut cx, ui);
            resize_edges(ctx, ui);
        });
        drop(cx);
        self.anims = anims;

        match menu_action {
            Some(MenuAction::Open) => self.open_image(ctx),
            Some(MenuAction::Settings) => self.settings_open = true,
            Some(MenuAction::Undo) => self.undo(),
            Some(MenuAction::Redo) => self.redo(),
            Some(MenuAction::Copy) => self.copy_adjustments(),
            Some(MenuAction::Paste) => self.paste_adjustments(),
            Some(MenuAction::Reset) => self.reset_adjustments(),
            Some(MenuAction::Shortcuts) => {
                self.settings_section = 0;
                self.settings_open = true;
            }
            None => {}
        }
        if std::mem::take(&mut self.export_requested) {
            self.export(ctx);
        }
        if self.sidebar_inactive() {
            self.export.menu_open = false;
        }
        self.crop_changed(previous_crop);

        // The GPU, brought up to date.
        self.sync_gpu();
        let layout = canvas::layout(self, &geo, ppp);
        let (cw, ch) = ((screen.width() * ppp).round().max(1.0), (screen.height() * ppp).round().max(1.0));
        if let (Some(l), Some(img)) = (&layout, &self.image) {
            let frame = view_frame(l, cw as f64, ch as f64, img.width, img.height);
            self.renderer.set_frame(frame);
            self.renderer.request_render_if_moved();
        }
        let measuring = self.debug_tools && self.debug.stats;
        let result = self.renderer.frame(measuring, colour_bar);
        if result.drew {
            self.last_frame_at = Some(now);
        }
        if result.canvas_replaced || (self.canvas_id.is_none() && self.renderer.canvas.is_some()) {
            if let Some(c) = &self.renderer.canvas {
                self.canvas_id = Some(self.gpu.register_canvas(c, self.canvas_id, wgpu::FilterMode::Nearest));
            }
        }
        let scrim = pal.scrim_strength;
        let frost_key = (backdrop, (scrim * 1000.0) as u32, (cw as u32, ch as u32), self.image.is_some());
        let frosted = if result.drew || self.frost_key != Some(frost_key) {
            self.frost_key = Some(frost_key);
            let canvas = if self.image.is_some() { self.renderer.canvas.as_ref() } else { None };
            let rgb = backdrop.to_normalized_gamma_f32();
            self.blur.run(&self.gpu, canvas, (cw as u32, ch as u32), ppp, [rgb[0], rgb[1], rgb[2]], scrim);
            true
        } else {
            false
        };

        let bg = ctx.layer_painter(egui::LayerId::new(Order::Background, Id::new("background")));
        bg.set(backdrop_slot, Shape::rect_filled(screen, 0.0, backdrop));
        if let (Some(id), true) = (self.canvas_id, self.image.is_some()) {
            bg.set(canvas_slot, Shape::image(id, screen, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE));
        }

        self.render_plots(now);
        ChromeResources::upload(&self.gpu, &fx, [cw, ch], scrim, frosted.then_some(&self.blur));

        // Repaint while anything moves, and wake for anything put off.
        let streaming = self.debug.stream.is_some_and(|(r, t)| r < t);
        if self.anims.running || tweening || self.renderer.wants_frame() || streaming || self.export.pending {
            ctx.request_repaint();
        } else if let Some(after) = result.repaint_after {
            ctx.request_repaint_after(after);
        }
        if let Some(due) = self.colour_space_debug.stats_due {
            ctx.request_repaint_after(due.saturating_duration_since(std::time::Instant::now()));
        }
    }
}

impl App {
    /// The live histogram, the curves backdrop and the F3 plots, each drawn
    /// again only when what they show changed.
    fn render_plots(&mut self, _now: f64) {
        let gpu = self.gpu.clone();
        if self.image.is_some() && self.histogram.has_source() && self.debug.histogram {
            let hist = (self.hist_size.0.round() as u32, self.hist_size.1.round() as u32);
            let curve = (self.curve_size.round() as u32, self.curve_size.round() as u32);
            let (_, a) = self.hist_plot.sized(&gpu, hist);
            let (_, b) = self.curve_plot.sized(&gpu, curve);
            let channel = if self.is_rgb { self.curve_channel } else { 0 };
            if a || b || self.histogram_dirty || self.drawn_channel != Some(channel) {
                self.drawn_channel = Some(channel);
                self.histogram_dirty = false;
                let payload = self.histogram_payload();
                let measure = self.debug_tools && self.debug.stats;
                let tx = self.tx.clone();
                let target = self.hist_plot.texture.clone();
                let backdrop = self.curve_plot.texture.clone();
                self.histogram.render(&gpu, &payload, target.as_ref(), backdrop.as_ref().map(|t| (t, channel)), measure, move |passes, record| {
                    let _ = tx.send(Msg::HistogramTimed(passes, record));
                });
            }
        }

        if !(self.debug_tools && self.colour_space_debug.enabled) {
            return;
        }
        let payload = self.histogram_payload();
        let Some(cs) = &mut self.colour_space else { return };
        let d = &self.colour_space_debug;
        if self.overlays_dirty {
            cs.set_overlays(&gpu, d.overlays, d.transformed, d.volume, gpu.display_space);
            self.overlays_dirty = false;
            self.colour_space_dirty = true;
        }
        let chroma = (self.chroma_size.0.round() as u32, self.chroma_size.1.round() as u32);
        let volume = (self.volume_size.0.round() as u32, self.volume_size.1.round() as u32);
        let (_, a) = self.chroma_plot.sized(&gpu, chroma);
        let (_, b) = self.volume_plot.sized(&gpu, volume);
        if a || b || self.colour_space_view_dirty {
            cs.set_volume_view(&gpu, d.yaw, d.pitch, volume.0 as f32 / volume.1.max(1) as f32);
        }
        let chroma_t = self.chroma_plot.texture.clone();
        let volume_t = d.volume.then(|| self.volume_plot.texture.clone()).flatten();
        if cs.has_source() && (a || b || self.colour_space_dirty) {
            self.colour_space_dirty = false;
            self.colour_space_view_dirty = false;
            let measure = self.debug_tools && self.debug.stats;
            let tx = self.tx.clone();
            cs.render(&gpu, &payload, chroma_t.as_ref(), volume_t.as_ref(), measure, move |passes, record| {
                let _ = tx.send(Msg::ColourSpaceTimed(passes, record));
            });
            // The tallies are read back once a drag has settled.
            self.colour_space_debug.stats_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(160));
        } else if self.colour_space_view_dirty {
            self.colour_space_view_dirty = false;
            cs.redraw(&gpu, chroma_t.as_ref(), volume_t.as_ref());
        }
        if self.colour_space_debug.stats_due.is_some_and(|t| std::time::Instant::now() >= t) {
            self.colour_space_debug.stats_due = None;
            let tx = self.tx.clone();
            cs.read_stats(&gpu, move |stats| {
                let _ = tx.send(Msg::GamutStats(stats));
            });
        }
    }
}
