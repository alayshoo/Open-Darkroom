// src-native/src/model/viewport.rs
//
// Where the image sits on the preview canvas, and which part of the image the
// render chain has to hold to show it. Mirrors src/lib/utils/viewport.ts.
// Canvas coordinates are device pixels from the window's top-left corner;
// image coordinates are full-resolution pixels.

use super::crop::{crop_view, rotated_bounds, rotation_canvas, upright_frame, Crop, CropView};

pub type Point = [f64; 2];
/// [x0, y0, x1, y1]
pub type BoxF = [f64; 4];

/// Most canvas pixels one image pixel may be enlarged to.
pub const MAX_ZOOM: f64 = 8.0;

/// Render pixels kept on every side of what is visible, so a short pan re-renders nothing.
pub const PAN_MARGIN: f64 = 192.0;

/// A point (x, y) mapped to `origin + x * axis_x + y * axis_y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub origin: Point,
    pub axis_x: Point,
    pub axis_y: Point,
}

impl From<&CropView> for Affine {
    fn from(v: &CropView) -> Self {
        Affine { origin: v.origin, axis_x: v.axis_x, axis_y: v.axis_y }
    }
}

pub fn apply(a: &Affine, [x, y]: Point) -> Point {
    [
        a.origin[0] + x * a.axis_x[0] + y * a.axis_y[0],
        a.origin[1] + x * a.axis_x[1] + y * a.axis_y[1],
    ]
}

/// `outer` after `inner`.
pub fn compose(outer: &Affine, inner: &Affine) -> Affine {
    let linear = |[x, y]: Point| -> Point {
        [x * outer.axis_x[0] + y * outer.axis_y[0], x * outer.axis_x[1] + y * outer.axis_y[1]]
    };
    Affine { origin: apply(outer, inner.origin), axis_x: linear(inner.axis_x), axis_y: linear(inner.axis_y) }
}

pub fn invert(a: &Affine) -> Affine {
    let ([a0, a1], [b0, b1]) = (a.axis_x, a.axis_y);
    let det = a0 * b1 - b0 * a1;
    let axis_x = [b1 / det, -a1 / det];
    let axis_y = [-b0 / det, a0 / det];
    let [ox, oy] = a.origin;
    Affine {
        origin: [-(ox * axis_x[0] + oy * axis_y[0]), -(ox * axis_x[1] + oy * axis_y[1])],
        axis_x,
        axis_y,
    }
}

/// (x, y) ↦ (x · sx + tx, y · sy + ty)
pub fn scale_by(sx: f64, sy: f64, tx: f64, ty: f64) -> Affine {
    Affine { origin: [tx, ty], axis_x: [sx, 0.0], axis_y: [0.0, sy] }
}

pub fn corners([x0, y0, x1, y1]: BoxF) -> [Point; 4] {
    [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
}

pub fn bounds(points: &[Point]) -> BoxF {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

/// `b` carried through `a`, as the box around where its corners land.
pub fn map_box(a: &Affine, b: BoxF) -> BoxF {
    bounds(&corners(b).map(|p| apply(a, p)))
}

pub fn intersect(a: BoxF, b: BoxF) -> Option<BoxF> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

pub fn contains(outer: BoxF, inner: BoxF) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

pub fn grow([x0, y0, x1, y1]: BoxF, by: f64) -> BoxF {
    [x0 - by, y0 - by, x1 + by, y1 + by]
}

// ── Placing the image ─────────────────────────────────────────────────────────

/// A crop's output laid on the canvas at `scale` canvas pixels per output
/// pixel, with output point `center` over canvas point `anchor`.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub view: CropView,
    pub scale: f64,
    pub center: Point,
    pub anchor: Point,
}

/// The box the image is fitted into, in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub center: Point,
    pub width: f64,
    pub height: f64,
}

pub fn canvas_to_output(p: &Placement) -> Affine {
    let s = 1.0 / p.scale;
    scale_by(s, s, p.center[0] - p.anchor[0] * s, p.center[1] - p.anchor[1] * s)
}

pub fn canvas_to_image(p: &Placement) -> Affine {
    compose(&Affine::from(&p.view), &canvas_to_output(p))
}

/// The largest scale that fits `width` × `height` into the slot, never past 1:1.
pub fn fit_scale(slot: &Slot, width: f64, height: f64) -> f64 {
    if !(slot.width > 0.0 && slot.height > 0.0) {
        return 1.0;
    }
    1f64.min(slot.width / width).min(slot.height / height)
}

pub fn clamp_center(center: Point, view: &CropView) -> Point {
    [center[0].clamp(0.0, view.width as f64), center[1].clamp(0.0, view.height as f64)]
}

/// The centre that keeps the output point under `cursor` where it is at a new scale.
pub fn zoom_about(p: &Placement, scale: f64, cursor: Point) -> Point {
    let [qx, qy] = apply(&canvas_to_output(p), cursor);
    [qx - (cursor[0] - p.anchor[0]) / scale, qy - (cursor[1] - p.anchor[1]) / scale]
}

/// What the canvas shows.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub placement: Placement,
    /// Canvas pixels outside this box are left clear, or None to draw all of
    /// the image that lands on the canvas.
    pub clip: Option<BoxF>,
    /// Canvas pixels outside this box are shaded.
    pub keep: Option<BoxF>,
    /// The scale that fits the image to the slot, which a zoom can never go below.
    pub fit: f64,
    /// The frame the crop tool's overlay is drawn over, in canvas pixels.
    pub frame: [f64; 2],
}

/// The crop on its own, fitted to the slot, or at `zoom` canvas pixels per
/// image pixel with output point `center` over the middle of the slot.
pub fn crop_layout(
    crop: &Crop,
    full_width: u32,
    full_height: u32,
    slot: &Slot,
    zoom: Option<f64>,
    center: Option<Point>,
) -> Layout {
    let view = crop_view(crop, full_width, full_height);
    let fit = fit_scale(slot, view.width as f64, view.height as f64);
    let placement = Placement {
        view,
        scale: zoom.unwrap_or(fit),
        center: match (zoom, center) {
            (Some(_), Some(c)) => clamp_center(c, &view),
            _ => [view.width as f64 / 2.0, view.height as f64 / 2.0],
        },
        anchor: slot.center,
    };
    Layout {
        placement,
        clip: Some(map_box(&invert(&canvas_to_output(&placement)), [0.0, 0.0, view.width as f64, view.height as f64])),
        keep: None,
        fit,
        frame: [view.width as f64 * fit, view.height as f64 * fit],
    }
}

/// All of the turned and rotated image the crop is cut from, laid out on its
/// upright frame, with everything outside the crop box shaded.
pub fn whole_layout(crop: &Crop, full_width: u32, full_height: u32, slot: &Slot) -> Layout {
    let (fw, fh) = (full_width as f64, full_height as f64);
    let view = crop_view(&rotation_canvas(crop, fw, fh), full_width, full_height);
    let reach = crop_view(&rotated_bounds(crop, fw, fh), full_width, full_height);
    let upright = crop_view(&upright_frame(crop), full_width, full_height);
    let scale = fit_scale(slot, reach.width as f64, reach.height as f64);
    let placement = Placement {
        view,
        scale,
        center: [view.width as f64 / 2.0, view.height as f64 / 2.0],
        anchor: slot.center,
    };
    let w = upright.width as f64 * scale;
    let h = upright.height as f64 * scale;
    let left = slot.center[0] - w / 2.0;
    let top = slot.center[1] - h / 2.0;
    Layout {
        placement,
        clip: None,
        keep: Some([
            left + (crop.cx - crop.w / 2.0) * w,
            top + (crop.cy - crop.h / 2.0) * h,
            left + (crop.cx + crop.w / 2.0) * w,
            top + (crop.cy + crop.h / 2.0) * h,
        ]),
        fit: scale,
        frame: [w, h],
    }
}

// ── What the renderer is asked to show ────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewFrame {
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub to_image: Affine,
    /// Canvas pixels per image pixel.
    pub scale: f64,
    pub clip: Option<BoxF>,
    pub keep: Option<BoxF>,
    pub full_width: f64,
    pub full_height: f64,
}

pub fn view_frame(layout: &Layout, canvas_width: f64, canvas_height: f64, full_width: u32, full_height: u32) -> ViewFrame {
    ViewFrame {
        canvas_width,
        canvas_height,
        to_image: canvas_to_image(&layout.placement),
        scale: layout.placement.scale,
        clip: layout.clip,
        keep: layout.keep,
        full_width: full_width as f64,
        full_height: full_height as f64,
    }
}

/// The part of the canvas the image is drawn on, and the part of the image it shows.
#[derive(Clone, Copy, Debug)]
pub struct Visible {
    pub canvas: BoxF,
    pub image: BoxF,
}

pub fn visible_region(frame: &ViewFrame) -> Option<Visible> {
    let image = [0.0, 0.0, frame.full_width, frame.full_height];
    let mut canvas = intersect(map_box(&invert(&frame.to_image), image), [0.0, 0.0, frame.canvas_width, frame.canvas_height])?;
    if let Some(clip) = frame.clip {
        canvas = intersect(canvas, clip)?;
    }
    let shown = intersect(map_box(&frame.to_image, canvas), image)?;
    Some(Visible { canvas, image: shown })
}

// ── The chain's region ────────────────────────────────────────────────────────

/// What the chain's textures hold: image pixel (x, y) is chain texel
/// (x · scale − box[0], y · scale − box[1]), and `apron` texels on every side
/// of the box only feed the blurs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainRegion {
    pub scale: f64,
    pub bx: BoxF,
    pub apron: f64,
}

/// The chain texels drawing `image` reads, at render scale `scale`.
pub fn needed_box(image: BoxF, scale: f64) -> BoxF {
    [
        (image[0] * scale).floor() - 1.0,
        (image[1] * scale).floor() - 1.0,
        (image[2] * scale).ceil() + 1.0,
        (image[3] * scale).ceil() + 1.0,
    ]
}

/// The part of a region whose pixels are fully rendered.
pub fn valid_box(region: &ChainRegion) -> BoxF {
    grow(region.bx, -region.apron)
}

/// The region to render to cover `needed`: `margin` more on every side as far
/// as the image goes, then the apron, never wider than `max_size`.
pub fn plan_region(
    needed: BoxF,
    scale: f64,
    apron: f64,
    margin: f64,
    full_width: f64,
    full_height: f64,
    max_size: f64,
) -> ChainRegion {
    let image = [-1.0, -1.0, (full_width * scale).ceil() + 1.0, (full_height * scale).ceil() + 1.0];
    let kept = intersect(grow(needed, margin), image).unwrap_or(needed);
    let mut bx = grow(kept, apron);
    bx[2] = bx[2].min(bx[0] + max_size);
    bx[3] = bx[3].min(bx[1] + max_size);
    ChainRegion { scale, bx, apron }
}

/// Whether a region rendered at another scale can stand in while a zoom is under way.
pub fn stands_in(region: &ChainRegion, scale: f64) -> bool {
    let ratio = scale / region.scale;
    (0.5..=2.0).contains(&ratio)
}

/// Canvas pixel → chain texel.
pub fn canvas_to_chain(frame: &ViewFrame, region: &ChainRegion) -> Affine {
    let s = region.scale;
    snap_to_texels(compose(&scale_by(s, s, -region.bx[0], -region.bx[1]), &frame.to_image))
}

/// Chain texel → texel of a source `source_width` × `source_height` covering the whole image.
pub fn chain_to_source(region: &ChainRegion, source_width: f64, source_height: f64, full_width: f64, full_height: f64) -> Affine {
    let kx = source_width / full_width / region.scale;
    let ky = source_height / full_height / region.scale;
    snap_to_texels(scale_by(kx, ky, region.bx[0] * kx, region.bx[1] * ky))
}

/// The image's own extent in chain texels.
pub fn image_in_chain(region: &ChainRegion, full_width: f64, full_height: f64) -> BoxF {
    let s = region.scale;
    [-region.bx[0], -region.bx[1], full_width * s - region.bx[0], full_height * s - region.bx[1]]
}

/// A map that steps one texel per pixel along the target's own axes is moved
/// to land on texel centres, so 1:1 shows the pixels themselves rather than
/// blends of neighbours.
pub fn snap_to_texels(a: Affine) -> Affine {
    let all = [a.axis_x[0], a.axis_x[1], a.axis_y[0], a.axis_y[1]];
    let unit = all.iter().all(|v| v.abs() < 1e-9 || (v.abs() - 1.0).abs() < 1e-9);
    if !unit {
        return a;
    }
    let round = |v: f64| if v.abs() < 1e-9 { 0.0 } else { v.signum() };
    Affine {
        origin: [js_round(a.origin[0]), js_round(a.origin[1])],
        axis_x: [round(a.axis_x[0]), round(a.axis_x[1])],
        axis_y: [round(a.axis_y[0]), round(a.axis_y[1])],
    }
}

/// `Math.round`, which rounds halves up rather than away from zero.
pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

// ── When to render ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailAction {
    Keep,
    Defer,
    Render,
}

/// Keep the sharp region held, put a fresh one off until the view stops, or
/// render it now. While the view moves, one at another scale stands in, and so
/// does any while a fresh render would cost more than a frame.
pub fn detail_action(held: Option<&ChainRegion>, stale: bool, image: BoxF, scale: f64, moving: bool, cheap: bool) -> DetailAction {
    let Some(held) = held else { return DetailAction::Render };
    if stale {
        return DetailAction::Render;
    }
    let covered = contains(valid_box(held), needed_box(image, held.scale));
    if covered && held.scale == scale {
        return DetailAction::Keep;
    }
    if !moving {
        return DetailAction::Render;
    }
    if (covered && stands_in(held, scale)) || !cheap {
        DetailAction::Defer
    } else {
        DetailAction::Render
    }
}

/// Whether a held region is worth drawing over the backdrop: never when shrunk
/// past half, where it would alias, nor when it is enlarged and the backdrop is
/// finer than it.
pub fn shows_detail(held: &ChainRegion, scale: f64, backdrop: Option<&ChainRegion>) -> bool {
    if held.scale > 2.0 * scale {
        return false;
    }
    held.scale >= scale || backdrop.is_none_or(|b| held.scale >= b.scale)
}

/// The scale the whole image is held at to fit `width` × `height` with an apron of `apron`.
pub fn backdrop_scale(full_width: f64, full_height: f64, source_scale: f64, width: f64, height: f64, apron: f64) -> f64 {
    1f64.min(source_scale)
        .min((width - 2.0 * apron - 4.0) / full_width)
        .min((height - 2.0 * apron - 4.0) / full_height)
}
