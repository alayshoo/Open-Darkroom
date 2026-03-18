// src-tauri/src/export_encoding.rs
//
// File-format-specific encoding: takes raw RGB bytes from the GPU pipeline
// and writes them to disk as PNG or JPEG with user-controlled quality settings.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::ImageEncoder;

// ── Export settings ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Png,
    Jpeg,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub format: ExportFormat,
    /// PNG deflate compression level, 0 (fastest/largest) – 9 (slowest/smallest).
    pub png_compression: u8,
    /// JPEG quality, 1 (lowest) – 100 (highest).
    pub jpeg_quality: u8,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Encode raw RGB bytes and save to `path` according to `settings`.
pub fn encode_and_save(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    settings: &ExportSettings,
    path: &Path,
) -> Result<(), String> {
    match settings.format {
        ExportFormat::Png  => encode_png(rgb_bytes, width, height, settings.png_compression, path),
        ExportFormat::Jpeg => encode_jpeg(rgb_bytes, width, height, settings.jpeg_quality, path),
    }
}

// ── PNG ───────────────────────────────────────────────────────────────────────

fn encode_png(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    compression: u8,
    path: &Path,
) -> Result<(), String> {
    let compression_type = match compression {
        0..=2 => CompressionType::Fast,
        3..=6 => CompressionType::Default,
        _     => CompressionType::Best,
    };

    let file = File::create(path).map_err(|e| e.to_string())?;
    let encoder = PngEncoder::new_with_quality(
        BufWriter::new(file),
        compression_type,
        FilterType::Adaptive,
    );
    encoder
        .write_image(&rgb_bytes, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())
}

// ── JPEG ──────────────────────────────────────────────────────────────────────

fn encode_jpeg(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    quality: u8,
    path: &Path,
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let encoder = JpegEncoder::new_with_quality(BufWriter::new(file), quality);
    encoder
        .write_image(&rgb_bytes, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())
}
