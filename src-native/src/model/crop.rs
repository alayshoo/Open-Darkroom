// src-native/src/model/crop.rs
//
// The crop helpers of src/lib/types/crop.ts. The crop itself, and the map from
// output pixels into the source, are crop.rs from the backend.

pub use crate::crop::{crop_view, CropPayload as Crop, CropView};

pub const FULL_FRAME: Crop = Crop::FULL_FRAME;

pub const MAX_ANGLE: f64 = std::f64::consts::FRAC_PI_4;

pub fn is_full_frame(c: &Crop) -> bool {
    *c == FULL_FRAME
}

/// The turned image before any rotation: the frame the crop tool lays out.
pub fn upright_frame(c: &Crop) -> Crop {
    Crop { quarter_turns: c.quarter_turns, ..FULL_FRAME }
}

/// The image's size once turned.
pub fn turned_size(c: &Crop, width: f64, height: f64) -> [f64; 2] {
    if c.quarter_turns % 2 == 0 {
        [width, height]
    } else {
        [height, width]
    }
}

/// All of the turned and rotated image, corners included.
pub fn rotated_bounds(c: &Crop, width: f64, height: f64) -> Crop {
    let [w, h] = turned_size(c, width, height);
    let cos = c.angle.cos().abs();
    let sin = c.angle.sin().abs();
    Crop {
        angle: c.angle,
        w: (cos * w + sin * h) / w,
        h: (sin * w + cos * h) / h,
        ..upright_frame(c)
    }
}

/// A canvas that holds the turned image at any angle, so rotating never
/// resizes it. The margins are whole pixels, so an unrotated image still lands
/// on texel centres.
pub fn rotation_canvas(c: &Crop, width: f64, height: f64) -> Crop {
    let [w, h] = turned_size(c, width, height);
    let diagonal = w.hypot(h);
    let margin_x = ((diagonal - w) / 2.0).ceil();
    let margin_y = ((diagonal - h) / 2.0).ceil();
    Crop {
        angle: c.angle,
        w: (w + 2.0 * margin_x) / w,
        h: (h + 2.0 * margin_y) / h,
        ..upright_frame(c)
    }
}

/// The same crop on the image turned a further quarter turn either way.
pub fn turn_crop(c: &Crop, clockwise: bool) -> Crop {
    Crop {
        cx: if clockwise { 1.0 - c.cy } else { c.cy },
        cy: if clockwise { c.cx } else { 1.0 - c.cx },
        w: c.h,
        h: c.w,
        angle: c.angle,
        quarter_turns: (c.quarter_turns + if clockwise { 1 } else { 3 }) % 4,
    }
}

// Slack on the inside test, in pixels, so a crop fitted exactly to the edge
// still counts as inside.
const INSIDE_SLACK: f64 = 1e-6;

/// Where a point of the rotated frame lands on the turned image, relative to
/// its centre, in pixels.
fn to_turned(x: f64, y: f64, size: [f64; 2], angle: f64) -> [f64; 2] {
    let dx = x - size[0] / 2.0;
    let dy = y - size[1] / 2.0;
    let (sin, cos) = angle.sin_cos();
    [cos * dx + sin * dy, -sin * dx + cos * dy]
}

/// Whether every corner of the crop falls on the image.
pub fn crop_inside(c: &Crop, width: f64, height: f64) -> bool {
    let size = turned_size(c, width, height);
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            let [x, y] = to_turned(
                (c.cx + sx * c.w / 2.0) * size[0],
                (c.cy + sy * c.h / 2.0) * size[1],
                size,
                c.angle,
            );
            if x.abs() > size[0] / 2.0 + INSIDE_SLACK || y.abs() > size[1] / 2.0 + INSIDE_SLACK {
                return false;
            }
        }
    }
    true
}

/// The largest copy of the crop, about the same centre and at the same aspect,
/// that stays on the image. A centre that has itself left the image is pulled
/// back onto it first.
pub fn fit_crop(c: &Crop, width: f64, height: f64) -> Crop {
    if crop_inside(c, width, height) {
        return *c;
    }
    let size = turned_size(c, width, height);
    let half = [size[0] / 2.0, size[1] / 2.0];
    let (sin, cos) = c.angle.sin_cos();

    let [mut dx, mut dy] = to_turned(c.cx * size[0], c.cy * size[1], size, c.angle);
    if dx.abs() >= half[0] || dy.abs() >= half[1] {
        dx = dx.clamp(-half[0], half[0]) * 0.999;
        dy = dy.clamp(-half[1], half[1]) * 0.999;
    }
    let cx = (half[0] + cos * dx - sin * dy) / size[0];
    let cy = (half[1] + sin * dx + cos * dy) / size[1];

    let a = c.w * size[0] / 2.0;
    let b = c.h * size[1] / 2.0;
    let mut scale: f64 = 1.0;
    for sx in [-1.0, 1.0] {
        for sy in [-1.0, 1.0] {
            let ex = cos * sx * a + sin * sy * b;
            let ey = -sin * sx * a + cos * sy * b;
            for (d, e, h) in [(dx, ex, half[0]), (dy, ey, half[1])] {
                if e > 0.0 {
                    scale = scale.min((h - d) / e);
                } else if e < 0.0 {
                    scale = scale.min((h + d) / -e);
                }
            }
        }
    }
    scale = scale.max(0.0);
    Crop { cx, cy, w: c.w * scale, h: c.h * scale, ..*c }
}

/// The `CropView` uniform, as chunks/cropView.wgsl declares it.
pub const CROP_VIEW_BYTES: usize = 80;

/// A box nothing falls outside, so it shades or clips nothing.
pub const EVERYWHERE: [f64; 4] = [-1e30, -1e30, 1e30, 1e30];

/// `keep` is the box, in output pixels, outside which the output is shaded.
/// Outside `image`, in source texels, and outside `clip`, in output pixels, the
/// output is left clear.
pub fn crop_view_floats(
    origin: [f64; 2],
    axis_x: [f64; 2],
    axis_y: [f64; 2],
    size: [f64; 2],
    keep: [f64; 4],
    image: [f64; 4],
    clip: [f64; 4],
) -> [u8; CROP_VIEW_BYTES] {
    let floats = [
        origin[0], origin[1], axis_x[0], axis_x[1], axis_y[0], axis_y[1], size[0], size[1],
        keep[0], keep[1], keep[2], keep[3], image[0], image[1], image[2], image[3], clip[0],
        clip[1], clip[2], clip[3],
    ];
    let mut bytes = [0u8; CROP_VIEW_BYTES];
    for (i, v) in floats.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&(*v as f32).to_le_bytes());
    }
    bytes
}
