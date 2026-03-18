// src-tauri/src/export_encoding.rs
//
// File-format-specific encoding: takes raw RGB bytes from the GPU pipeline
// and writes them to disk as PNG, JPEG, WebP, or TIFF with user-controlled
// quality settings.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::tiff::TiffEncoder;
use image::ImageEncoder;

// ── Export settings ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Webp,
    Tiff,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub format: ExportFormat,
    /// PNG deflate compression level, 0 (fastest/largest) – 9 (slowest/smallest).
    pub png_compression: u8,
    /// JPEG quality, 1 (lowest) – 100 (highest).
    pub jpeg_quality: u8,
    /// WebP: encode losslessly when true.
    pub webp_lossless: bool,
    /// WebP lossy quality, 1 (lowest) – 100 (highest).
    pub webp_quality: u8,
    /// WebP lossless compression effort, 0 (fastest) – 9 (smallest).
    pub webp_compression: u8,
    /// TIFF bit depth per channel: 8 or 16.
    pub tiff_bit_depth: u8,
    /// Whether the image is in black-and-white mode (greyscale output for TIFF).
    pub is_bw: bool,
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
        ExportFormat::Webp => encode_webp(rgb_bytes, width, height, settings.webp_lossless, settings.webp_quality, settings.webp_compression, path),
        ExportFormat::Tiff => encode_tiff(rgb_bytes, width, height, settings.tiff_bit_depth, settings.is_bw, path),
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

// ── WebP ──────────────────────────────────────────────────────────────────────

fn encode_webp(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    lossless: bool,
    quality: u8,
    compression: u8,
    path: &Path,
) -> Result<(), String> {
    let encoder = webp::Encoder::from_rgb(&rgb_bytes, width, height);
    let mem: webp::WebPMemory = if lossless {
        // Set lossless mode and map the 0-9 slider to libwebp's method (0-6).
        let mut config = webp::WebPConfig::new()
            .map_err(|_| "Failed to create WebP config".to_string())?;
        config.lossless = 1;
        config.method = (compression as i32 * 6 / 9).clamp(0, 6);
        encoder
            .encode_advanced(&config)
            .map_err(|e| format!("WebP lossless encode failed: {e:?}"))?
    } else {
        encoder.encode(quality as f32)
    };
    std::fs::write(path, &*mem).map_err(|e| e.to_string())
}

// ── TIFF ──────────────────────────────────────────────────────────────────────

fn encode_tiff(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    bit_depth: u8,
    is_bw: bool,
    path: &Path,
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let writer = BufWriter::new(file);

    match (bit_depth, is_bw) {
        (8, false) => {
            TiffEncoder::new(writer)
                .write_image(&rgb_bytes, width, height, image::ExtendedColorType::Rgb8)
                .map_err(|e| e.to_string())
        }
        (8, true) => {
            // In BW mode R=G=B; take the R channel as the grey value.
            let grey: Vec<u8> = rgb_bytes.chunks_exact(3).map(|px| px[0]).collect();
            TiffEncoder::new(writer)
                .write_image(&grey, width, height, image::ExtendedColorType::L8)
                .map_err(|e| e.to_string())
        }
        (16, false) => {
            // render_image already produced 6 bytes/pixel as u16 LE — pass straight through.
            TiffEncoder::new(writer)
                .write_image(&rgb_bytes, width, height, image::ExtendedColorType::Rgb16)
                .map_err(|e| e.to_string())
        }
        (16, true) => {
            // Extract just the R channel (first 2 bytes of each 6-byte pixel — already u16 LE).
            let grey16: Vec<u8> = rgb_bytes
                .chunks_exact(6)
                .flat_map(|px| [px[0], px[1]])
                .collect();
            TiffEncoder::new(writer)
                .write_image(&grey16, width, height, image::ExtendedColorType::L16)
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("Unsupported TIFF bit depth: {}", bit_depth)),
    }
}
