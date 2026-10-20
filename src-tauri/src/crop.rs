// src-tauri/src/crop.rs
//
// The crop as the frontend stores it, and the map from output pixels back into
// the source that colorSpaceEncode.wgsl samples through. Mirrors
// src/lib/types/crop.ts.

/// Size of the `CropView` uniform declared in chunks/cropView.wgsl.
pub const CROP_VIEW_BYTES: usize = 32;

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CropPayload {
    /// Centre, as fractions of the full image's width and height.
    pub cx: f64,
    pub cy: f64,
    /// Extent, as fractions of the full image's width and height.
    pub w: f64,
    pub h: f64,
    /// Radians, clockwise on screen.
    pub angle: f64,
}

impl CropPayload {
    pub const FULL_FRAME: Self = Self { cx: 0.5, cy: 0.5, w: 1.0, h: 1.0, angle: 0.0 };

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

/// An unrotated crop is snapped to whole source pixels, so every output pixel
/// lands on a texel centre and the encode stage reads it back untouched.
pub fn crop_view(crop: &CropPayload, src_width: u32, src_height: u32) -> CropView {
    let crop = if crop.is_valid() { *crop } else { CropPayload::FULL_FRAME };

    if crop.angle == 0.0 {
        let snap = |centre: f64, extent: f64, size: u32| {
            let size_f = size as f64;
            let lo = ((centre - extent / 2.0) * size_f).round().clamp(0.0, size_f - 1.0);
            let hi = ((centre + extent / 2.0) * size_f).round().clamp(lo + 1.0, size_f);
            (lo, (hi - lo) as u32)
        };
        let (x0, width) = snap(crop.cx, crop.w, src_width);
        let (y0, height) = snap(crop.cy, crop.h, src_height);
        return CropView {
            width,
            height,
            origin: [x0, y0],
            axis_x: [1.0, 0.0],
            axis_y: [0.0, 1.0],
        };
    }

    let width = (crop.w * src_width as f64).round().max(1.0);
    let height = (crop.h * src_height as f64).round().max(1.0);
    let (sin, cos) = crop.angle.sin_cos();
    let centre_x = crop.cx * src_width as f64;
    let centre_y = crop.cy * src_height as f64;
    CropView {
        width: width as u32,
        height: height as u32,
        origin: [
            centre_x - width / 2.0 * cos + height / 2.0 * sin,
            centre_y - width / 2.0 * sin - height / 2.0 * cos,
        ],
        axis_x: [cos, sin],
        axis_y: [-sin, cos],
    }
}

impl CropView {
    pub fn source_point(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.origin[0] + x * self.axis_x[0] + y * self.axis_y[0],
            self.origin[1] + x * self.axis_x[1] + y * self.axis_y[1],
        ]
    }

    /// Source texels that bilinear sampling of output rows `rows` reads, as a
    /// half-open `[x0, x1) × [y0, y1)` before any clamping to the image.
    pub fn source_texels(&self, rows: std::ops::Range<u32>) -> [i64; 4] {
        let (top, bottom) = (rows.start as f64 + 0.5, rows.end as f64 - 0.5);
        let (left, right) = (0.5, self.width as f64 - 0.5);
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
    /// and from output row `first_row` onwards.
    pub fn shifted(&self, offset: [f64; 2], first_row: u32) -> CropView {
        let start = self.source_point(0.0, first_row as f64);
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
