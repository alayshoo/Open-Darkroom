// src-tauri/src/crop.rs
//
// The crop as the frontend stores it, and the map from output pixels back into
// the source that colorSpaceEncode.wgsl samples through.

/// Size of the `CropView` uniform declared in chunks/cropView.wgsl.
pub const CROP_VIEW_BYTES: usize = 32;

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CropPayload {
    /// Centre, as fractions of the turned image's width and height.
    pub cx: f64,
    pub cy: f64,
    /// Extent, as fractions of the turned image's width and height.
    pub w: f64,
    pub h: f64,
    /// Radians the image is rotated clockwise on screen, about its centre.
    pub angle: f64,
    /// Clockwise quarter turns, applied before `angle`.
    #[serde(default)]
    pub quarter_turns: u32,
}

impl CropPayload {
    pub const FULL_FRAME: Self =
        Self { cx: 0.5, cy: 0.5, w: 1.0, h: 1.0, angle: 0.0, quarter_turns: 0 };

    /// The whole frame is what anything malformed falls back to.
    fn is_valid(&self) -> bool {
        [self.cx, self.cy, self.w, self.h, self.angle].iter().all(|v| v.is_finite())
            && self.w > 0.0
            && self.h > 0.0
    }
}

impl Default for CropPayload {
    fn default() -> Self {
        Self::FULL_FRAME
    }
}

/// Output pixels mapped back into a source of a given size: the source texel
/// coordinate of output point (x, y) is `origin + x * axis_x + y * axis_y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropView {
    pub width: u32,
    pub height: u32,
    pub origin: [f64; 2],
    pub axis_x: [f64; 2],
    pub axis_y: [f64; 2],
}

/// The image is turned, then rotated about its centre, with the box upright in
/// that frame. Unrotated crops snap to whole pixels so they sample texel centres.
/// Mirrors `cropView()` in src/lib/types/crop.ts.
pub fn crop_view(crop: &CropPayload, src_width: u32, src_height: u32) -> CropView {
    let crop = if crop.is_valid() { *crop } else { CropPayload::FULL_FRAME };
    let turns = crop.quarter_turns % 4;
    let (src_w, src_h) = (src_width as f64, src_height as f64);
    let size = if turns % 2 == 0 { [src_w, src_h] } else { [src_h, src_w] };

    let (x0, y0, width, height);
    // A frame reaching past the image is never snapped: snapping clamps to it.
    if crop.angle == 0.0 && crop.w <= 1.0 && crop.h <= 1.0 {
        let snap = |centre: f64, extent: f64, size: f64| {
            let lo = ((centre - extent / 2.0) * size).round().clamp(0.0, size - 1.0);
            let hi = ((centre + extent / 2.0) * size).round().clamp(lo + 1.0, size);
            (lo, hi - lo)
        };
        (x0, width) = snap(crop.cx, crop.w, size[0]);
        (y0, height) = snap(crop.cy, crop.h, size[1]);
    } else {
        width = (crop.w * size[0]).round().max(1.0);
        height = (crop.h * size[1]).round().max(1.0);
        x0 = crop.cx * size[0] - width / 2.0;
        y0 = crop.cy * size[1] - height / 2.0;
    }

    // Rotated frame → turned image → source.
    let (sin, cos) = crop.angle.sin_cos();
    let to_source = |x: f64, y: f64| -> [f64; 2] {
        let (dx, dy) = (x - size[0] / 2.0, y - size[1] / 2.0);
        let ox = cos * dx + sin * dy + size[0] / 2.0;
        let oy = -sin * dx + cos * dy + size[1] / 2.0;
        match turns {
            1 => [oy, src_h - ox],
            2 => [src_w - ox, src_h - oy],
            3 => [src_w - oy, ox],
            _ => [ox, oy],
        }
    };

    let origin = to_source(x0, y0);
    let right = to_source(x0 + 1.0, y0);
    let down = to_source(x0, y0 + 1.0);
    CropView {
        width: width as u32,
        height: height as u32,
        origin,
        axis_x: [right[0] - origin[0], right[1] - origin[1]],
        axis_y: [down[0] - origin[0], down[1] - origin[1]],
    }
}

impl CropView {
    pub fn source_point(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.origin[0] + x * self.axis_x[0] + y * self.axis_y[0],
            self.origin[1] + x * self.axis_x[1] + y * self.axis_y[1],
        ]
    }

    /// Whether output rows and columns run along the source's own axes.
    pub fn is_axis_aligned(&self) -> bool {
        self.axis_x[0] * self.axis_x[1] == 0.0 && self.axis_y[0] * self.axis_y[1] == 0.0
    }

    /// Source texels that bilinear sampling of the output block `cols × rows`
    /// reads, as a half-open `[x0, x1) × [y0, y1)` before any clamping.
    pub fn source_texels(
        &self,
        cols: std::ops::Range<u32>,
        rows: std::ops::Range<u32>,
    ) -> [i64; 4] {
        let (top, bottom) = (rows.start as f64 + 0.5, rows.end as f64 - 0.5);
        let (left, right) = (cols.start as f64 + 0.5, cols.end as f64 - 0.5);
        let corners = [
            self.source_point(left, top),
            self.source_point(right, top),
            self.source_point(left, bottom),
            self.source_point(right, bottom),
        ];

        // Texel centres sit at +0.5, so a point's lower neighbour is floor(p - 0.5)
        // and its upper one the texel after ceil(p - 0.5).
        let lo = |axis: usize| {
            corners.iter().map(|c| c[axis] - 0.5).fold(f64::INFINITY, f64::min).floor() as i64
        };
        let hi = |axis: usize| {
            corners.iter().map(|c| c[axis] - 0.5).fold(f64::NEG_INFINITY, f64::max).ceil() as i64
                + 1
        };
        [lo(0), lo(1), hi(0), hi(1)]
    }

    /// Whether the source pixel at (x, y) falls inside the crop, judged at its
    /// centre.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let dx = x as f64 + 0.5 - self.origin[0];
        let dy = y as f64 + 0.5 - self.origin[1];
        let u = dx * self.axis_x[0] + dy * self.axis_x[1];
        let v = dx * self.axis_y[0] + dy * self.axis_y[1];
        u >= 0.0 && u < self.width as f64 && v >= 0.0 && v < self.height as f64
    }

    /// The same view, as seen from a region of the source starting at `offset`
    /// and from output pixel `first` onwards.
    pub fn shifted(&self, offset: [f64; 2], first: [u32; 2]) -> CropView {
        let start = self.source_point(first[0] as f64, first[1] as f64);
        CropView {
            origin: [start[0] - offset[0], start[1] - offset[1]],
            ..*self
        }
    }
}

/// Pack the view into the uniform matching the WGSL `CropView` struct.
pub fn crop_view_to_bytes(view: &CropView) -> [u8; CROP_VIEW_BYTES] {
    let floats = [
        view.origin[0],
        view.origin[1],
        view.axis_x[0],
        view.axis_x[1],
        view.axis_y[0],
        view.axis_y[1],
        view.width as f64,
        view.height as f64,
    ];
    let mut bytes = [0u8; CROP_VIEW_BYTES];
    for (i, v) in floats.iter().enumerate() {
        bytes[i * 4..i * 4 + 4].copy_from_slice(&(*v as f32).to_le_bytes());
    }
    bytes
}
