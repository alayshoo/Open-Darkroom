// src-tauri/src/thumbnails.rs
//
// Small renders of an image with its edit, through the same WGSL as export.
// Free of Tauri, like export_rendering.rs.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;

use crate::color::{to_linear_u16, LUT_SIZE};
use crate::crop::CropPayload;
use crate::export_rendering::{chunk_rows_for_margin, render_cropped_in_view, sharp_margin_rows, SlidersPayload, ViewPayload};
use crate::icc::SourceSpace;
use crate::image_opening::{decode_path, downscale_to_max_edge};
use crate::output_space::OutputSpace;
use crate::retouch::RetouchPayload;

/// Longest edge of a stored thumbnail: a grid cell on a high density display.
pub const THUMBNAIL_EDGE: u32 = 768;

const JPEG_QUALITY: u8 = 85;

/// An image shrunk to thumbnail size, before any edit.
pub struct ThumbnailSource {
    pub path: PathBuf,
    pub pixels: Vec<u16>,
    pub width: u32,
    pub height: u32,
    /// The size of the image it was shrunk from.
    pub full_width: u32,
    pub full_height: u32,
    pub channels: usize,
    pub space: SourceSpace,
}

impl ThumbnailSource {
    /// Shrunk in linear light, as the preview is, so fine texture keeps its brightness.
    pub fn from_pixels(
        path: &Path,
        pixels: &[u16],
        width: u32,
        height: u32,
        channels: usize,
        space: SourceSpace,
    ) -> Result<Self, String> {
        let baked = space.bake(pixels, channels)?;
        let luts = space.luts(|v| (v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32).round() as u16);
        let linear = to_linear_u16(baked.as_deref().unwrap_or(pixels), channels, &luts);
        let (pixels, shrunk_width, shrunk_height) = downscale_to_max_edge(&linear, width, height, channels, THUMBNAIL_EDGE);
        Ok(Self {
            path: path.to_path_buf(),
            pixels,
            width: shrunk_width,
            height: shrunk_height,
            full_width: width,
            full_height: height,
            channels,
            space: SourceSpace::linear_with_matrix(space.to_working),
        })
    }

    /// Decodes the file and keeps only the shrunk copy.
    pub fn decode(path: &Path) -> Result<Self, String> {
        let (pixels, channels, width, height, space) = decode_path(path)?;
        Self::from_pixels(path, &pixels, width, height, channels, space)
    }
}

/// The source last rendered from, so re-rendering an image as it is edited
/// does not shrink it again each time.
static LAST_SOURCE: Mutex<Option<Arc<ThumbnailSource>>> = Mutex::new(None);

pub fn cached_source(path: &Path) -> Option<Arc<ThumbnailSource>> {
    LAST_SOURCE.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|s| s.path == path)
}

pub fn keep_source(source: Arc<ThumbnailSource>) {
    *LAST_SOURCE.lock().unwrap_or_else(|e| e.into_inner()) = Some(source);
}

/// The edit rendered over the source in sRGB, encoded as a JPEG.
pub async fn render_thumbnail(
    source: &ThumbnailSource,
    sliders: &SlidersPayload,
    crop: &CropPayload,
    retouch: &[RetouchPayload],
) -> Result<Vec<u8>, String> {
    let view = ViewPayload {
        render_scale: source.width as f32 / source.full_width.max(1) as f32,
        full_width: source.full_width as f32,
        full_height: source.full_height as f32,
    };
    let rendered = render_cropped_in_view(
        &source.pixels,
        source.width,
        source.height,
        source.channels,
        sliders,
        crop,
        retouch,
        &source.space,
        OutputSpace::Srgb,
        8,
        chunk_rows_for_margin(sharp_margin_rows(sliders, source.width, source.height)),
        view,
        |_| {},
    )
    .await?;
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode(&rendered.rgb, rendered.width, rendered.height, ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())?;
    Ok(jpeg)
}
