// src-native/src/app.rs
//
// The app's state and what changes it: +page.svelte's script, with the
// backend commands it invoked running here on worker threads instead.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use crate::crop::crop_view;
use crate::export_encoding::{encode_and_save, ExportFormat, ExportSettings};
use crate::export_rendering::{render_cropped_image, RolloffMode, ToneMapPayload};
use crate::full_resolution::FullResolution;
use crate::gpu::chrome::Blur;
use crate::gpu::color_space::{ColorSpace, GamutStats};
use crate::gpu::frame_timer::PassTiming;
use crate::gpu::histogram::{Histogram, Plot};
use crate::gpu::renderer::{FrameSample, Renderer};
use crate::gpu::source::{self, band_rows, Mipmapper, SourceImage};
use crate::gpu::Gpu;
use crate::icc::{Matrix3, SourceSpace};
use crate::image_opening::{compute_histograms_in, decode_file, open_extensions, prepare_image, PreparedImage, HIST_BINS};
use crate::model::clipboard;
use crate::model::crop::{is_full_frame, turn_crop, Crop, FULL_FRAME};
use crate::model::curves::{bake_lut, Curves};
use crate::model::history::{Action, History};
use crate::model::settings;
use crate::model::shortcuts::{Press, ShortcutId, Shortcuts};
use crate::model::sliders::{self, hsl_keys, mode_keys, Key, Sliders, Value, OVERRIDES_BW, SLIDER_TOLERANCE};
use crate::output_space::OutputSpace;
use crate::ui::anim::Anims;
use crate::ui::widgets::Editing;

pub type Bins = [[u32; HIST_BINS]; 3];

/// The open image, as the backend's state held it.
pub struct OpenImage {
    pub id: u32,
    pub pixels: Option<Arc<Vec<u16>>>,
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub space: SourceSpace,
    pub full_resolution: Arc<FullResolution>,
    pub histograms: Bins,
    pub to_working: Matrix3,
    pub overview: Vec<u8>,
    pub overview_size: (u32, u32),
}

pub enum Msg {
    /// A file was picked; the image held so far is let go before decoding.
    Picked,
    Cancelled,
    Opened {
        id: u32,
        prepared: Box<PreparedImage>,
        pixels: Vec<u16>,
        channels: usize,
        space: SourceSpace,
        elapsed_ms: u128,
    },
    OpenFailed(String),
    FullResolution { id: u32, image: SourceImage, elapsed_ms: u128 },
    StreamProgress { id: u32, rows: u32, total: u32 },
    CropHistograms { request: u32, bins: Bins },
    ExportStarted,
    Exported(Result<(), String>),
    HistogramTimed(Vec<PassTiming>, f64),
    ColourSpaceTimed(Vec<PassTiming>, f64),
    GamutStats(GamutStats),
}

/// One update that runs on its own cadence: its GPU passes and the CPU that
/// recorded them, kept apart since they do not overlap in time.
#[derive(Clone, Default)]
pub struct UpdateSample {
    pub passes: Vec<PassTiming>,
    pub record_ms: f64,
}

/// The F3 panel's state, kept for the session.
pub struct ColourSpaceDebug {
    pub enabled: bool,
    pub overlays: [bool; 4],
    pub transformed: bool,
    pub volume: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub rolloff: f32,
    pub rolloff_mode: RolloffMode,
    pub stats: Option<GamutStats>,
    pub stats_due: Option<Instant>,
}

pub struct ExportUi {
    pub settings: ExportSettingsUi,
    pub pending: bool,
    pub modal: bool,
    pub menu_open: bool,
    /// Set on the frame the arrow toggled the menu, so that click does not
    /// also count as one outside it.
    pub menu_toggled: bool,
    pub progress: Arc<AtomicU32>,
    /// What the format last defaulted from, so a change of format re-picks the space.
    pub defaulted_for: (ExportFormat, u8),
}

#[derive(Clone, Copy)]
pub struct ExportSettingsUi {
    pub format: ExportFormat,
    pub output_space: OutputSpace,
    pub png_compression: u8,
    pub jpeg_quality: u8,
    pub webp_lossless: bool,
    pub webp_quality: u8,
    pub webp_compression: u8,
    pub tiff_bit_depth: u8,
}

impl Default for ExportSettingsUi {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            output_space: OutputSpace::Srgb,
            png_compression: 6,
            jpeg_quality: 90,
            webp_lossless: true,
            webp_quality: 90,
            webp_compression: 6,
            tiff_bit_depth: 8,
        }
    }
}

/// The space a format is written in unless the choice is overridden: sRGB
/// for eight bits, the working space for a 16-bit TIFF.
pub fn default_output_space(format: ExportFormat, depth: u8) -> OutputSpace {
    if matches!(format, ExportFormat::Tiff) && depth == 16 {
        OutputSpace::Rec2020
    } else {
        OutputSpace::Srgb
    }
}

/// A numeric slider on its way to a new value, as animateObject moves them.
struct SliderTween {
    index: usize,
    from: f64,
    to: f64,
    start: f64,
    duration: f64,
}

pub struct Debug {
    pub stats: bool,
    pub histogram: bool,
    pub frame_samples: Arc<std::sync::Mutex<Vec<FrameSample>>>,
    pub histogram_samples: Vec<UpdateSample>,
    pub colour_space_samples: Vec<UpdateSample>,
    pub stream: Option<(u32, u32)>,
}

pub struct App {
    pub gpu: Gpu,
    pub renderer: Renderer,
    pub histogram: Histogram,
    pub colour_space: Option<ColorSpace>,
    pub mipmapper: Mipmapper,
    pub blur: Blur,
    pub canvas_id: Option<egui::TextureId>,
    pub hist_plot: Plot,
    pub curve_plot: Plot,
    pub chroma_plot: Plot,
    pub volume_plot: Plot,
    pub icons: crate::ui::icons::Icons,
    pub anims: Anims,
    pub start: Instant,

    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    next_image_id: u32,
    pub loading_id: Arc<AtomicU32>,

    pub is_darkroom: bool,
    pub mode_slide_until: f64,
    pub page_normal: usize,
    pub pager_offset: f32,
    pub pager_idle_at: f64,
    pub page_scroll: [f32; 4],
    pub sliders: Sliders,
    tweens: Vec<SliderTween>,
    pub curves: Curves,
    pub curve_channel: usize,
    pub is_rgb: bool,
    saved_colour_values: Option<Vec<(usize, f64)>>,
    pub colour_animating_until: f64,
    pub hsl_band: usize,
    /// Heights of the sections B&W folds away, measured while shown.
    pub slide_heights: std::collections::HashMap<&'static str, f32>,
    /// The live histogram's size in pixels, which its texture follows.
    pub hist_size: (f32, f32),
    pub curve_size: f32,
    pub export_requested: bool,
    /// When the renderer last drew, for the stats overlay's idle reading.
    pub last_frame_at: Option<f64>,
    pub card_heights: std::collections::HashMap<String, f32>,
    pub colour_bar_scroll: f32,
    pub chroma_size: (f32, f32),
    pub volume_size: (f32, f32),
    pub overlays_dirty: bool,
    pub frost_key: Option<(egui::Color32, u32, (u32, u32), bool)>,
    pub drawn_channel: Option<usize>,

    pub image: Option<OpenImage>,
    pub crop: Crop,
    pub draft_crop: Crop,
    pub is_cropping: bool,
    pub is_straightening: bool,
    pub zoom: Option<f64>,
    pub zoom_center: Option<[f64; 2]>,
    pub zoomable: bool,
    pub input_histograms: Option<Bins>,
    histogram_request: u32,

    pub export: ExportUi,
    pub proofing: bool,
    pub history: History,
    pub shortcuts: Shortcuts,
    pub debug_tools: bool,
    pub settings_open: bool,
    pub settings_section: usize,
    pub shortcut_notice: Option<(ShortcutId, String)>,
    pub debug: Debug,
    pub colour_space_debug: ColourSpaceDebug,
    pub menu_open: Option<usize>,
    pub menu_switching: bool,
    pub title: String,
    pub editing: Editing,

    // What the GPU was last told, so only changes are uploaded.
    sent_payload: Option<crate::export_rendering::SlidersPayload>,
    sent_curves: Option<Option<Vec<f32>>>,
    sent_tone: Option<ToneMapPayload>,
    sent_output: Option<Option<OutputSpace>>,
    pub histogram_dirty: bool,
    pub colour_space_dirty: bool,
    pub colour_space_view_dirty: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.as_ref().expect("the wgpu renderer");
        let gpu = Gpu::new(rs);
        crate::ui::theme::install_fonts(&cc.egui_ctx);
        let blur = Blur::new(&gpu);
        crate::gpu::chrome::ChromeResources::install(&gpu, &blur);
        let persisted = settings::load();
        let (tx, rx) = channel();
        let renderer = Renderer::new(&gpu);
        let frame_samples = renderer.samples.clone();
        let startup_path = std::env::args_os().nth(1).map(PathBuf::from);
        let mut app = Self {
            histogram: Histogram::new(&gpu),
            colour_space: None,
            mipmapper: Mipmapper::new(&gpu),
            blur,
            renderer,
            canvas_id: None,
            hist_plot: Plot::new(),
            curve_plot: Plot::new(),
            chroma_plot: Plot::new(),
            volume_plot: Plot::new(),
            icons: crate::ui::icons::Icons::load(&cc.egui_ctx),
            anims: Anims::default(),
            start: Instant::now(),
            tx,
            rx,
            next_image_id: 0,
            loading_id: Arc::new(AtomicU32::new(0)),
            is_darkroom: false,
            mode_slide_until: 0.0,
            page_normal: 1,
            pager_offset: 1.0,
            pager_idle_at: 0.0,
            page_scroll: [0.0; 4],
            sliders: Sliders::default_rgb(),
            tweens: Vec::new(),
            curves: Curves::default(),
            curve_channel: 0,
            is_rgb: true,
            saved_colour_values: None,
            colour_animating_until: 0.0,
            hsl_band: 0,
            slide_heights: Default::default(),
            hist_size: (1.0, 1.0),
            curve_size: 1.0,
            export_requested: false,
            last_frame_at: None,
            card_heights: Default::default(),
            colour_bar_scroll: 0.0,
            chroma_size: (1.0, 1.0),
            volume_size: (1.0, 1.0),
            overlays_dirty: true,
            frost_key: None,
            drawn_channel: None,
            image: None,
            crop: FULL_FRAME,
            draft_crop: FULL_FRAME,
            is_cropping: false,
            is_straightening: false,
            zoom: None,
            zoom_center: None,
            zoomable: false,
            input_histograms: None,
            histogram_request: 0,
            export: ExportUi {
                settings: ExportSettingsUi::default(),
                pending: false,
                modal: false,
                menu_open: false,
                menu_toggled: false,
                progress: Arc::new(AtomicU32::new(0)),
                defaulted_for: (ExportFormat::Png, 8),
            },
            proofing: false,
            history: History::default(),
            shortcuts: Shortcuts::load(&persisted.shortcuts),
            debug_tools: persisted.debug_tools,
            settings_open: false,
            settings_section: 0,
            shortcut_notice: None,
            debug: Debug {
                // Frame stats start visible in a dev build and hidden in a
                // release one, as the Tauri app's do.
                stats: cfg!(debug_assertions),
                histogram: true,
                frame_samples,
                histogram_samples: Vec::new(),
                colour_space_samples: Vec::new(),
                stream: None,
            },
            colour_space_debug: ColourSpaceDebug {
                enabled: false,
                overlays: [true, false, false, true],
                transformed: false,
                volume: true,
                yaw: 0.6,
                pitch: 0.42,
                rolloff: 0.0,
                rolloff_mode: RolloffMode::Luminance,
                stats: None,
                stats_due: None,
            },
            menu_open: None,
            menu_switching: false,
            title: "Open Darkroom".into(),
            editing: Editing::default(),
            sent_payload: None,
            sent_curves: None,
            sent_tone: None,
            sent_output: None,
            histogram_dirty: true,
            colour_space_dirty: true,
            colour_space_view_dirty: false,
            gpu,
        };
        if let Some(path) = startup_path {
            app.open_path(&cc.egui_ctx, path);
        }
        app
    }

    pub fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    // ── Derived state ────────────────────────────────────────────────────────

    /// The adjustments have nothing to act on without an image, and are set
    /// aside while the crop tool has the preview.
    pub fn sidebar_inactive(&self) -> bool {
        self.image.is_none() || self.is_cropping
    }

    pub fn active_keys(&self) -> Vec<Key> {
        mode_keys(self.is_darkroom)
    }

    pub fn curve_lut(&self) -> Option<Vec<f32>> {
        bake_lut(&self.curves, self.is_rgb)
    }

    pub fn tone_map(&self) -> ToneMapPayload {
        if !self.debug_tools {
            return ToneMapPayload::default();
        }
        ToneMapPayload { rolloff: self.colour_space_debug.rolloff, rolloff_mode: self.colour_space_debug.rolloff_mode }
    }

    pub fn proof_space(&self) -> Option<OutputSpace> {
        self.proofing.then_some(self.export.settings.output_space)
    }

    fn is_at_default(&self, key: Key) -> bool {
        let d = Sliders::default_rgb();
        match (self.sliders.get(key), d.get(key)) {
            (Value::Num(v), Value::Num(def)) => (v - def).abs() <= SLIDER_TOLERANCE,
            (a, b) => a == b,
        }
    }

    pub fn is_colour_mixer_identity(&self) -> bool {
        hsl_keys().all(|i| self.is_at_default(Key::N(i)))
    }

    /// Whether a reset would change anything, judged on the keys it writes.
    pub fn is_identity(&self) -> bool {
        let animating = self.now() < self.colour_animating_until;
        (self.is_darkroom || self.curves == Curves::default())
            && self.active_keys().into_iter().all(|k| {
                let overridden = matches!(k, Key::N(i) if sliders::is_bw_override(i));
                ((!self.is_rgb || animating) && overridden) || self.is_at_default(k)
            })
    }

    // ── Slider animation ─────────────────────────────────────────────────────

    /// Move numeric sliders to new values over `duration`, eased out.
    pub fn animate_sliders(&mut self, targets: &[(usize, f64)], duration: f64) {
        let now = self.now();
        self.tweens.retain(|t| !targets.iter().any(|(i, _)| *i == t.index));
        for &(index, to) in targets {
            self.tweens.push(SliderTween { index, from: self.sliders.v[index], to, start: now, duration });
        }
    }

    pub fn step_tweens(&mut self) -> bool {
        let now = self.now();
        for t in &self.tweens {
            let x = ((now - t.start) / t.duration).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - x).powi(3);
            self.sliders.v[t.index] = t.from + (t.to - t.from) * eased;
        }
        self.tweens.retain(|t| now - t.start < t.duration);
        !self.tweens.is_empty()
    }

    // ── Modes ────────────────────────────────────────────────────────────────

    pub fn toggle_mode(&mut self) {
        self.is_darkroom = !self.is_darkroom;
        self.mode_slide_until = self.now() + 0.5;
    }

    pub fn toggle_colour_mode(&mut self) {
        self.is_rgb = !self.is_rgb;
        const COLOUR_ANIM: f64 = 0.2;
        if !self.is_rgb {
            self.saved_colour_values = Some(OVERRIDES_BW.iter().map(|&(k, _)| (k, self.sliders.v[k])).collect());
            self.animate_sliders(&OVERRIDES_BW, COLOUR_ANIM);
            self.colour_animating_until = self.now() + COLOUR_ANIM;
        } else if let Some(saved) = self.saved_colour_values.take() {
            self.animate_sliders(&saved, COLOUR_ANIM);
            self.colour_animating_until = self.now() + COLOUR_ANIM;
        }
    }

    pub fn toggle_invert(&mut self) {
        self.sliders.invert = !self.sliders.invert;
        self.history.push(Action::Slider {
            key: Key::Invert,
            old: Value::Bool(!self.sliders.invert),
            new: Value::Bool(self.sliders.invert),
        });
    }

    pub fn page_count(&self) -> usize {
        if self.is_darkroom {
            1
        } else {
            3
        }
    }

    pub fn active_page(&self) -> usize {
        if self.is_darkroom {
            0
        } else {
            self.page_normal
        }
    }

    pub fn scroll_to_page(&mut self, page: usize) {
        if !self.is_darkroom {
            self.page_normal = page.min(2);
        }
    }

    pub fn move_page(&mut self, right: bool) {
        let page = self.active_page();
        let next = if right { (page + 1).min(self.page_count() - 1) } else { page.saturating_sub(1) };
        self.scroll_to_page(next);
    }

    // ── History ──────────────────────────────────────────────────────────────

    pub fn commit(&mut self, action: Action) {
        self.history.push(action);
    }

    pub fn commit_slider(&mut self, index: usize, old: f64, new: f64) {
        self.commit(Action::Slider { key: Key::N(index), old: Value::Num(old), new: Value::Num(new) });
    }

    /// Numbers travel to their new values the way the colour-mode toggle
    /// moves them; anything else lands at once.
    fn set_sliders(&mut self, values: &[(Key, Value)]) {
        let mut numeric = Vec::new();
        for &(k, v) in values {
            match (k, v) {
                (Key::N(i), Value::Num(x)) => numeric.push((i, x)),
                _ => self.sliders.set(k, v),
            }
        }
        self.animate_sliders(&numeric, 0.2);
    }

    fn set_slider(&mut self, key: Key, value: Value) {
        if let Key::N(i) = key {
            self.tweens.retain(|t| t.index != i);
        }
        self.sliders.set(key, value);
    }

    fn apply(&mut self, action: &Action, undo: bool) {
        match action {
            Action::Slider { key, old, new } => self.set_slider(*key, if undo { *old } else { *new }),
            Action::Sliders { old, new, curves } => {
                self.set_sliders(if undo { old } else { new });
                if let Some((o, n)) = curves {
                    self.curves = if undo { o.clone() } else { n.clone() };
                }
            }
            Action::Curves { old, new } => self.curves = if undo { old.clone() } else { new.clone() },
            Action::Crop { old, new } => {
                self.crop = if undo { *old } else { *new };
                self.cancel_crop();
            }
        }
    }

    /// Undo or redo inside the crop tool first throws away an unapplied drag.
    fn leave_crop_for_history(&mut self) -> bool {
        if !self.is_cropping {
            return false;
        }
        let discarded = self.draft_crop != self.crop;
        self.cancel_crop();
        discarded
    }

    pub fn undo(&mut self) {
        if self.leave_crop_for_history() {
            return;
        }
        if let Some(a) = self.history.undo() {
            self.apply(&a, true);
        }
    }

    pub fn redo(&mut self) {
        if self.leave_crop_for_history() {
            return;
        }
        if let Some(a) = self.history.redo() {
            self.apply(&a, false);
        }
    }

    // ── Adjustments clipboard ────────────────────────────────────────────────

    /// In B&W the colour keys are pinned, so an incoming set puts its colour
    /// values aside for the toggle back to colour. The whole set lands as one
    /// history step, with any curves it carries.
    fn apply_slider_set(&mut self, values: Vec<(Key, Value)>, curves: Vec<(usize, Vec<crate::model::curves::Point>)>) {
        let mut next = values;
        if !self.is_rgb {
            let mut held = self.saved_colour_values.take().unwrap_or_default();
            next.retain(|&(k, v)| {
                if let (Key::N(i), Value::Num(x)) = (k, v) {
                    if sliders::is_bw_override(i) {
                        held.retain(|(j, _)| *j != i);
                        held.push((i, x));
                        return false;
                    }
                }
                true
            });
            self.saved_colour_values = Some(held);
        }
        let old: Vec<(Key, Value)> = next.iter().map(|&(k, _)| (k, self.sliders.get(k))).collect();
        let old_curves = self.curves.clone();
        let mut new_curves = old_curves.clone();
        for (c, points) in curves {
            new_curves.0[c] = points;
        }
        let curves_change = (old_curves != new_curves).then(|| (old_curves, new_curves.clone()));
        self.commit(Action::Sliders { old, new: next.clone(), curves: curves_change.clone() });
        self.set_sliders(&next);
        if curves_change.is_some() {
            self.curves = new_curves;
        }
    }

    pub fn copy_adjustments(&mut self) {
        let keys = self.active_keys();
        let curves = (!self.is_darkroom).then_some(&self.curves);
        let text = clipboard::write(&self.sliders, &keys, curves);
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
            eprintln!("Failed to copy adjustments: {e}");
        }
    }

    pub fn paste_adjustments(&mut self) {
        if self.sidebar_inactive() {
            return;
        }
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) => t,
            Err(e) => return eprintln!("Failed to paste adjustments: {e}"),
        };
        match clipboard::read(&text, &self.active_keys(), !self.is_darkroom) {
            Ok(p) => self.apply_slider_set(p.sliders, p.curves),
            Err(e) => eprintln!("Failed to paste adjustments: {e}"),
        }
    }

    /// Always the colour defaults: in B&W the overridden keys are diverted to
    /// the saved colour values, leaving the mode itself untouched.
    fn reset_keys(&mut self, keys: Vec<Key>, curves: bool) {
        let d = Sliders::default_rgb();
        let values = keys.into_iter().map(|k| (k, d.get(k))).collect();
        let curve_values = if curves {
            Curves::default().0.into_iter().enumerate().collect()
        } else {
            Vec::new()
        };
        self.apply_slider_set(values, curve_values);
    }

    pub fn reset_adjustments(&mut self) {
        if self.sidebar_inactive() {
            return;
        }
        let keys = self.active_keys();
        self.reset_keys(keys, !self.is_darkroom);
    }

    pub fn reset_colour_mixer(&mut self) {
        self.reset_keys(hsl_keys().map(Key::N).collect(), false);
    }

    // ── Crop ─────────────────────────────────────────────────────────────────

    pub fn enter_crop(&mut self) {
        self.zoom = None;
        self.draft_crop = self.crop;
        self.is_cropping = true;
    }

    pub fn apply_crop(&mut self) {
        self.is_cropping = false;
        self.is_straightening = false;
        if self.draft_crop == self.crop {
            return;
        }
        self.zoom = None;
        self.commit(Action::Crop { old: self.crop, new: self.draft_crop });
        self.crop = self.draft_crop;
    }

    pub fn cancel_crop(&mut self) {
        self.is_cropping = false;
        self.is_straightening = false;
    }

    pub fn toggle_crop(&mut self) {
        if self.image.is_none() {
            return;
        }
        if self.is_cropping {
            self.apply_crop();
        } else {
            self.enter_crop();
        }
    }

    pub fn turn_draft(&mut self, clockwise: bool) {
        self.draft_crop = turn_crop(&self.draft_crop, clockwise);
    }

    pub fn toggle_zoom(&mut self) {
        self.zoom = if self.zoom.is_none() { Some(1.0) } else { None };
    }

    /// The darkroom's per-channel histograms follow the crop, so a film border
    /// left outside it stops weighing on the black and white points.
    fn request_crop_histograms(&mut self) {
        self.histogram_request += 1;
        let Some(img) = &self.image else {
            self.input_histograms = None;
            return;
        };
        if is_full_frame(&self.crop) {
            self.input_histograms = Some(img.histograms);
            return;
        }
        let Some(pixels) = img.pixels.clone() else { return };
        let (w, h, ch, crop) = (img.width, img.height, img.channels, self.crop);
        let (tx, request) = (self.tx.clone(), self.histogram_request);
        std::thread::spawn(move || {
            let view = crop_view(&crop, w, h);
            let hist = compute_histograms_in(&pixels, w, h, ch, &view);
            let _ = tx.send(Msg::CropHistograms { request, bins: [hist.r, hist.g, hist.b] });
        });
    }

    // ── Opening ──────────────────────────────────────────────────────────────

    pub fn open_image(&mut self, ctx: &egui::Context) {
        let pick = move || {
            let extensions = open_extensions();
            rfd::FileDialog::new().add_filter("Images", &extensions).pick_file()
        };
        // macOS only shows a dialog from the main thread.
        let picked: Option<Option<PathBuf>> = if cfg!(target_os = "macos") { Some(pick()) } else { None };
        self.decode(ctx, move || picked.unwrap_or_else(pick));
    }

    /// Open a file named up front, as on the command line.
    pub fn open_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.decode(ctx, move || Some(path));
    }

    fn decode(&mut self, ctx: &egui::Context, path: impl FnOnce() -> Option<PathBuf> + Send + 'static) {
        self.next_image_id += 1;
        let id = self.next_image_id;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let Some(path) = path() else {
                let _ = tx.send(Msg::Cancelled);
                return;
            };
            let _ = tx.send(Msg::Picked);
            ctx.request_repaint();
            let started = Instant::now();
            let result = decode_file(&path).and_then(|(pixels, channels, w, h, space)| {
                let prepared = prepare_image(&pixels, w, h, channels, &space)?;
                Ok((prepared, pixels, channels, space))
            });
            let _ = tx.send(match result {
                Ok((prepared, pixels, channels, space)) => Msg::Opened {
                    id,
                    prepared: Box::new(prepared),
                    pixels,
                    channels,
                    space,
                    elapsed_ms: started.elapsed().as_millis(),
                },
                Err(e) => Msg::OpenFailed(e),
            });
            ctx.request_repaint();
        });
    }

    fn on_opened(&mut self, id: u32, prepared: PreparedImage, pixels: Vec<u16>, channels: usize, space: SourceSpace) {
        let gpu = self.gpu.clone();
        let preview = source::upload(&gpu, Some(&self.mipmapper), "Preview Source", &prepared.preview_pixels, prepared.preview_width, prepared.preview_height);
        self.renderer.load_image(preview, &prepared.to_working);
        let overview = || source::upload(&gpu, None, "Overview", &prepared.overview_pixels, prepared.overview_width, prepared.overview_height);
        self.histogram.set_source(&gpu, overview(), &prepared.to_working);
        if let Some(cs) = &mut self.colour_space {
            cs.set_source(&gpu, overview(), &prepared.to_working);
        }
        let h = &prepared.histograms;
        let pixels = Arc::new(pixels);
        self.image = Some(OpenImage {
            id,
            pixels: Some(pixels.clone()),
            width: prepared.full_width,
            height: prepared.full_height,
            channels,
            space: space.clone(),
            full_resolution: Arc::default(),
            histograms: [h.r, h.g, h.b],
            to_working: prepared.to_working,
            overview: prepared.overview_pixels.clone(),
            overview_size: (prepared.overview_width, prepared.overview_height),
        });
        self.zoom = None;
        self.zoom_center = None;
        // A crop describes one frame, so neither it nor its history carries
        // over the way the adjustments do.
        self.crop = FULL_FRAME;
        self.cancel_crop();
        self.history.discard(|a| matches!(a, Action::Crop { .. }));
        self.request_crop_histograms();
        self.histogram_dirty = true;
        self.colour_space_dirty = true;
        self.renderer.request_render();

        // An image no larger than the preview already has every pixel.
        self.loading_id.store(id, Ordering::SeqCst);
        if (prepared.preview_width, prepared.preview_height) == (prepared.full_width, prepared.full_height) {
            self.debug.stream = None;
            return;
        }
        self.stream_full_resolution(id, pixels, prepared.full_width, prepared.full_height, channels, space);
    }

    /// The full resolution, streamed a band of rows at a time straight into
    /// one texture from a worker thread, its mip levels built once it is in.
    fn stream_full_resolution(&mut self, id: u32, pixels: Arc<Vec<u16>>, width: u32, height: u32, channels: usize, space: SourceSpace) {
        let gpu = self.gpu.clone();
        let tx = self.tx.clone();
        let current = self.loading_id.clone();
        let full = self.image.as_ref().unwrap().full_resolution.clone();
        self.debug.stream = Some((0, height));
        std::thread::spawn(move || {
            let started = Instant::now();
            let max_edge = gpu.max_texture;
            let (sw, sh) = crate::full_resolution::served_size(width, height, max_edge);
            let rows = band_rows(sw);
            let texture = source::source_texture(&gpu, "Full Resolution Source", sw, sh);
            let mut y = 0;
            while y < sh {
                if current.load(Ordering::SeqCst) != id {
                    return;
                }
                let band = match full.rows(&pixels, width, height, channels, &space, max_edge, y, rows) {
                    Ok(b) => b,
                    Err(e) => return eprintln!("Failed to load the full-resolution image: {e}"),
                };
                source::write_rows(&gpu, &texture, band.width, band.y, band.rows, &band.pixels);
                y = band.y + band.rows;
                let _ = tx.send(Msg::StreamProgress { id, rows: y, total: sh });
            }
            let image = SourceImage { texture, width: sw, height: sh };
            let _ = tx.send(Msg::FullResolution { id, image, elapsed_ms: started.elapsed().as_millis() });
        });
    }

    // ── Export ───────────────────────────────────────────────────────────────

    pub fn export(&mut self, ctx: &egui::Context) {
        if self.export.pending {
            return;
        }
        self.export.menu_open = false;
        if self.is_cropping {
            self.apply_crop();
        }
        let Some(img) = &self.image else { return };
        let Some(pixels) = img.pixels.clone() else { return };
        self.export.pending = true;
        let s = self.export.settings;
        let settings = ExportSettings {
            format: s.format,
            output_space: s.output_space,
            png_compression: s.png_compression,
            jpeg_quality: s.jpeg_quality,
            webp_lossless: s.webp_lossless,
            webp_quality: s.webp_quality,
            webp_compression: s.webp_compression,
            tiff_bit_depth: s.tiff_bit_depth,
            is_bw: !self.is_rgb,
        };
        let lut = self.curve_lut();
        let payload = self.sliders.payload(self.tone_map(), lut.as_deref());
        let (w, h, ch, space, crop) = (img.width, img.height, img.channels, img.space.clone(), self.crop);
        let (tx, progress, ctx) = (self.tx.clone(), self.export.progress.clone(), ctx.clone());
        progress.store(0f32.to_bits(), Ordering::SeqCst);
        let (filter, extensions, ext): (&str, &[&str], &str) = match settings.format {
            ExportFormat::Png => ("PNG Image", &["png"], "png"),
            ExportFormat::Jpeg => ("JPEG Image", &["jpg", "jpeg"], "jpg"),
            ExportFormat::Webp => ("WebP Image", &["webp"], "webp"),
            ExportFormat::Tiff => ("TIFF Image", &["tiff", "tif"], "tiff"),
        };
        let pick = move || rfd::FileDialog::new().add_filter(filter, extensions).save_file();
        let picked: Option<Option<PathBuf>> = if cfg!(target_os = "macos") { Some(pick()) } else { None };
        std::thread::spawn(move || {
            let Some(mut path) = picked.unwrap_or_else(pick) else {
                let _ = tx.send(Msg::Exported(Err("No file selected".into())));
                ctx.request_repaint();
                return;
            };
            if path.extension().map(|e| e.to_ascii_lowercase()) != Some(ext.into()) {
                path.set_extension(ext);
            }
            let _ = tx.send(Msg::ExportStarted);
            ctx.request_repaint();
            let bit_depth = if matches!(settings.format, ExportFormat::Tiff) { settings.tiff_bit_depth } else { 8 };
            let started = Instant::now();
            let report = {
                let (progress, ctx) = (progress.clone(), ctx.clone());
                move |p: f32| {
                    progress.store(p.to_bits(), Ordering::SeqCst);
                    ctx.request_repaint();
                }
            };
            let result = pollster::block_on(render_cropped_image(&pixels, w, h, ch, &payload, &crop, &space, settings.output_space, bit_depth, report))
                .and_then(|r| encode_and_save(r.rgb, r.width, r.height, &settings, &path));
            progress.store(1f32.to_bits(), Ordering::SeqCst);
            println!("export: {} in {} ms", path.display(), started.elapsed().as_millis());
            let _ = tx.send(Msg::Exported(result));
            ctx.request_repaint();
        });
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    pub fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Picked => {
                    // Let go of the held buffer before the next one is decoded.
                    if let Some(img) = &mut self.image {
                        img.pixels = None;
                    }
                    self.loading_id.store(0, Ordering::SeqCst);
                }
                Msg::Cancelled => {}
                Msg::Opened { id, prepared, pixels, channels, space, elapsed_ms } => {
                    if id == self.next_image_id {
                        println!("open: decoded and prepared in {elapsed_ms} ms");
                        self.on_opened(id, *prepared, pixels, channels, space);
                    }
                }
                Msg::OpenFailed(e) => {
                    // A failed open has already cost the image it was holding.
                    eprintln!("Failed to open image: {e}");
                    self.image = None;
                    self.renderer.clear_image();
                    self.histogram.clear();
                    self.input_histograms = None;
                }
                Msg::StreamProgress { id, rows, total } => {
                    if self.image.as_ref().is_some_and(|i| i.id == id) {
                        self.debug.stream = Some((rows, total));
                    }
                }
                Msg::FullResolution { id, image, elapsed_ms } => {
                    if self.image.as_ref().is_some_and(|i| i.id == id) {
                        self.mipmapper.generate(&self.gpu, &image.texture);
                        println!("Full resolution {}×{} on the GPU in {elapsed_ms} ms", image.width, image.height);
                        self.renderer.set_source(image);
                        self.renderer.request_render();
                    }
                }
                Msg::CropHistograms { request, bins } => {
                    if request == self.histogram_request {
                        self.input_histograms = Some(bins);
                    }
                }
                Msg::ExportStarted => self.export.modal = true,
                Msg::Exported(result) => {
                    if let Err(e) = result {
                        if e != "No file selected" {
                            eprintln!("Export failed: {e}");
                        }
                    }
                    self.export.pending = false;
                    self.export.modal = false;
                }
                Msg::HistogramTimed(passes, record_ms) => push_sample(&mut self.debug.histogram_samples, UpdateSample { passes, record_ms }),
                Msg::ColourSpaceTimed(passes, record_ms) => push_sample(&mut self.debug.colour_space_samples, UpdateSample { passes, record_ms }),
                Msg::GamutStats(stats) => self.colour_space_debug.stats = Some(stats),
            }
        }
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    pub fn run_shortcut(&mut self, ctx: &egui::Context, id: ShortcutId) {
        match id {
            ShortcutId::PagePrev => self.move_page(false),
            ShortcutId::PageNext => self.move_page(true),
            ShortcutId::Undo => self.undo(),
            ShortcutId::Redo => self.redo(),
            ShortcutId::ChangeMode => self.toggle_mode(),
            ShortcutId::OpenImage => self.open_image(ctx),
            ShortcutId::CopyAdjustments => self.copy_adjustments(),
            ShortcutId::PasteAdjustments => self.paste_adjustments(),
            ShortcutId::ResetAdjustments => self.reset_adjustments(),
            ShortcutId::ToggleCrop => self.toggle_crop(),
            // The diagnostics hang off the settings master switch.
            ShortcutId::ToggleDebugStats => {
                if self.debug_tools {
                    self.debug.stats = !self.debug.stats;
                    if !self.debug.stats {
                        self.clear_debug();
                    }
                }
            }
            ShortcutId::ToggleHistogram => {
                if self.debug_tools {
                    self.debug.histogram = !self.debug.histogram;
                    self.clear_debug();
                }
            }
            ShortcutId::ToggleColorSpace => {
                if self.debug_tools {
                    self.colour_space_debug.enabled = !self.colour_space_debug.enabled;
                }
            }
            ShortcutId::ToggleSettings => self.settings_open = !self.settings_open,
        }
    }

    pub fn clear_debug(&mut self) {
        self.debug.frame_samples.lock().unwrap().clear();
        self.debug.histogram_samples.clear();
        self.debug.colour_space_samples.clear();
    }

    pub fn set_debug_tools(&mut self, on: bool) {
        self.debug_tools = on;
        settings::save("debugTools", serde_json::Value::Bool(on));
        self.debug.stats = on;
        if !on {
            self.debug.histogram = true;
            self.colour_space_debug.enabled = false;
        }
        self.clear_debug();
    }

    /// The window's key presses, routed to the crop tool, a recording shortcut
    /// row, the settings modal or the shortcut registry.
    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        let text_focused = ctx.wants_keyboard_input();
        for e in events {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            let press = Press {
                key: key_name(key, modifiers.shift),
                ctrl: modifiers.ctrl,
                shift: modifiers.shift,
                alt: modifiers.alt,
                meta: modifiers.mac_cmd,
            };
            if let Some(recording) = self.shortcuts.recording {
                // Escape always dismisses, so it is worth more as the way out.
                if key != egui::Key::Escape && !press.key.is_empty() {
                    let binding = crate::model::shortcuts::serialize(&press.binding());
                    let stolen = self.shortcuts.add(recording, &binding);
                    let notice = if stolen.is_empty() { String::new() } else { format!("Taken from {}.", stolen.join(", ")) };
                    self.shortcut_notice = Some((recording, notice));
                }
                self.shortcuts.recording = None;
                continue;
            }
            let id = self.shortcuts.action_for(&press);
            if self.settings_open {
                if key == egui::Key::Escape || id == Some(ShortcutId::ToggleSettings) {
                    self.settings_open = false;
                }
                continue;
            }
            if self.menu_open.is_some() {
                self.menu_open = None;
            }
            // Enter and Escape belong to the crop tool while it is open.
            if self.is_cropping && !self.export.menu_open && !text_focused && (key == egui::Key::Enter || key == egui::Key::Escape) {
                if key == egui::Key::Enter {
                    self.apply_crop();
                } else if self.is_straightening {
                    self.is_straightening = false;
                } else {
                    self.cancel_crop();
                }
                continue;
            }
            let Some(id) = id else { continue };
            // A field with the caret in it owns plain typing and the clipboard pair.
            if text_focused {
                if matches!(id, ShortcutId::CopyAdjustments | ShortcutId::PasteAdjustments) {
                    continue;
                }
                if press.key.chars().count() == 1 && !press.ctrl && !press.alt && !press.meta {
                    continue;
                }
            }
            self.run_shortcut(ctx, id);
        }
    }

    // ── GPU sync ─────────────────────────────────────────────────────────────

    /// Hand the adjustments to whatever renders them, if they changed.
    pub fn sync_gpu(&mut self) {
        let lut = self.curve_lut();
        let tone = self.tone_map();
        let payload = self.sliders.payload(tone, None);
        let proof = self.proof_space();
        let display = self.gpu.display_space;
        if self.sent_output != Some(proof) {
            self.renderer.set_output_space(display, proof);
            self.histogram.set_output_space(&self.gpu, display, proof);
            if let Some(cs) = &self.colour_space {
                cs.set_output_space(&self.gpu, display, proof);
            }
            self.sent_output = Some(proof);
            self.renderer.request_render();
            self.histogram_dirty = true;
            self.colour_space_dirty = true;
        }
        if self.sent_tone != Some(tone) {
            self.renderer.set_tone_map(&tone);
            self.histogram.set_tone_map(&tone);
            if let Some(cs) = &self.colour_space {
                cs.set_tone_map(&tone);
            }
            self.sent_tone = Some(tone);
            self.renderer.request_render();
            self.histogram_dirty = true;
            self.colour_space_dirty = true;
        }
        if self.sent_curves.as_ref() != Some(&lut) {
            self.renderer.set_curves(lut.as_deref());
            self.histogram.set_curves(lut.as_deref());
            if let Some(cs) = &self.colour_space {
                cs.set_curves(lut.as_deref());
            }
            self.sent_curves = Some(lut);
            self.renderer.request_render();
            self.histogram_dirty = true;
            self.colour_space_dirty = true;
        }
        if self.sent_payload.as_ref() != Some(&payload) {
            self.renderer.set_sliders(payload.clone());
            self.sent_payload = Some(payload);
            self.renderer.request_render();
            self.histogram_dirty = true;
            self.colour_space_dirty = true;
        }
    }

    pub fn histogram_payload(&self) -> crate::export_rendering::SlidersPayload {
        self.sent_payload.clone().unwrap_or_default()
    }

    /// The F3 panel's pipeline, made the first time it is opened.
    pub fn ensure_colour_space(&mut self) {
        if self.colour_space.is_some() {
            return;
        }
        let mut cs = ColorSpace::new(&self.gpu);
        let lut = self.curve_lut();
        cs.set_curves(lut.as_deref());
        cs.set_tone_map(&self.tone_map());
        cs.set_output_space(&self.gpu, self.gpu.display_space, self.proof_space());
        if let Some(img) = &self.image {
            let overview = source::upload(&self.gpu, None, "Overview", &img.overview, img.overview_size.0, img.overview_size.1);
            cs.set_source(&self.gpu, overview, &img.to_working);
        }
        self.colour_space = Some(cs);
        self.colour_space_dirty = true;
    }

    pub fn crop_changed(&mut self, previous: Crop) {
        if previous != self.crop {
            self.request_crop_histograms();
            self.histogram.set_crop(self.crop);
            self.histogram_dirty = true;
        }
    }
}

fn push_sample(list: &mut Vec<UpdateSample>, s: UpdateSample) {
    list.push(s);
    let excess = list.len().saturating_sub(180);
    list.drain(..excess);
}

/// A key as KeyboardEvent.key spells it, which is what bindings are written in.
pub fn key_name(key: egui::Key, shift: bool) -> String {
    use egui::Key as K;
    let s = match key {
        K::Comma => ",",
        K::Period => ".",
        K::Minus => "-",
        K::Plus => "+",
        K::Equals => "=",
        K::Slash => "/",
        K::Backslash => "\\",
        K::Semicolon => ";",
        K::Quote => "'",
        K::OpenBracket => "[",
        K::CloseBracket => "]",
        K::Backtick => "`",
        K::Space => " ",
        K::Escape => "Escape",
        K::Enter => "Enter",
        K::Tab => "Tab",
        K::Backspace => "Backspace",
        K::Delete => "Delete",
        K::ArrowUp => "ArrowUp",
        K::ArrowDown => "ArrowDown",
        K::ArrowLeft => "ArrowLeft",
        K::ArrowRight => "ArrowRight",
        K::Home => "Home",
        K::End => "End",
        K::PageUp => "PageUp",
        K::PageDown => "PageDown",
        K::Insert => "Insert",
        _ => {
            let name = key.name();
            return if name.chars().count() == 1 {
                if shift {
                    name.to_uppercase()
                } else {
                    name.to_lowercase()
                }
            } else {
                name.to_string()
            };
        }
    };
    s.to_string()
}
