// src-tauri/src/export_encoding.rs
//
// File-format-specific encoding: takes raw RGB bytes from the GPU pipeline
// and writes them to disk as PNG, JPEG, WebP, TIFF or JPEG XL with user-controlled
// quality settings.
//
// Every file is tagged with the profile of the space it was rendered into.
// Untagged 8-bit is read as sRGB by most software and would have been right by
// accident; nothing would have rescued an untagged wide-gamut or 16-bit file.

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::ImageEncoder;
use tiff::encoder::colortype::{self, ColorType};
use tiff::encoder::{TiffEncoder, TiffValue};
use tiff::tags::{Tag, Type};

use crate::jxl_encoding::{encode_jxl, JxlOptions};
use crate::output_space::OutputSpace;

// ── Export settings ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Jxl,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub format: ExportFormat,
    /// The colour space the image is rendered into and the file is tagged with.
    pub output_space: OutputSpace,
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
    /// JPEG XL: encode losslessly when true.
    pub jxl_lossless: bool,
    /// JPEG XL lossy quality, 1 (lowest) – 100 (highest); 90 is visually lossless.
    pub jxl_quality: u8,
    /// JPEG XL encoder effort, 1 (fastest) – 9 (smallest).
    pub jxl_effort: u8,
    /// JPEG XL lossless bits per channel, 8 – 16. Lossy is always 16.
    pub jxl_bit_depth: u8,
    /// Whether the image is in black-and-white mode (greyscale output for TIFF
    /// and JPEG XL).
    pub is_bw: bool,
}

impl ExportSettings {
    /// Bits per channel the render has to produce for this format.
    pub fn bit_depth(&self) -> u8 {
        match self.format {
            ExportFormat::Tiff => self.tiff_bit_depth,
            // Anything past 8 bits is rendered at 16 and requantised by libjxl.
            ExportFormat::Jxl if self.jxl_depth() > 8 => 16,
            _ => 8,
        }
    }

    /// Bits per channel the JPEG XL file declares. Lossy quantises far coarser
    /// than any input depth, so it takes the full 16 bits at no cost in size.
    fn jxl_depth(&self) -> u8 {
        if self.jxl_lossless { self.jxl_bit_depth } else { 16 }
    }

    /// Whether the encoder reports how much of the file it has written.
    pub fn reports_encode_progress(&self) -> bool {
        matches!(self.format, ExportFormat::Png | ExportFormat::Tiff)
    }

    /// Rough encode time in seconds per megapixel, measured on four cores with
    /// grainy content. It only decides how much of the progress bar encoding
    /// gets, so it needs to be the right order of magnitude, not exact.
    pub fn encode_seconds_per_megapixel(&self) -> f32 {
        let quality = |q: u8| f32::from(q.clamp(1, 100)) / 100.0;
        match self.format {
            ExportFormat::Tiff => 0.005,
            ExportFormat::Jpeg => 0.02 + 0.03 * quality(self.jpeg_quality),
            ExportFormat::Png if self.png_compression <= 2 => 0.015,
            ExportFormat::Png => 0.14,
            ExportFormat::Webp if self.webp_lossless && self.webp_compression == 0 => 0.17,
            ExportFormat::Webp if self.webp_lossless => 0.5,
            ExportFormat::Webp => 0.08 + 0.05 * quality(self.webp_quality),
            ExportFormat::Jxl => {
                // Indexed by effort; the even efforts sit between measured odd ones.
                const LOSSY: [f32; 9] = [0.04, 0.04, 0.04, 0.07, 0.10, 0.14, 0.17, 0.22, 0.27];
                const LOSSLESS_8: [f32; 9] = [0.008, 0.03, 0.048, 0.09, 0.134, 0.25, 0.376, 0.8, 1.39];
                const LOSSLESS_16: [f32; 9] = [0.023, 0.04, 0.056, 0.11, 0.164, 0.3, 0.48, 1.5, 2.9];

                let e = usize::from(self.jxl_effort.clamp(1, 9) - 1);
                let seconds = if self.jxl_lossless {
                    let t = f32::from(self.jxl_bit_depth.clamp(8, 16) - 8) / 8.0;
                    LOSSLESS_8[e] + (LOSSLESS_16[e] - LOSSLESS_8[e]) * t
                } else {
                    LOSSY[e]
                };
                // libjxl is the one encoder here that uses every core.
                let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
                seconds * 4.0 / cores as f32
            }
        }
    }

    /// Whether the file is written with a single channel.
    fn greyscale(&self) -> bool {
        self.is_bw && matches!(self.format, ExportFormat::Tiff | ExportFormat::Jxl)
    }
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
    encode_and_save_with_progress(rgb_bytes, width, height, settings, path, &mut |_| {})
}

/// As [`encode_and_save`], calling `on_progress` with the fraction written so
/// far for the formats where [`ExportSettings::reports_encode_progress`] holds.
///
/// The file is written next to `path` and moved over it only once complete, so
/// a failed export never leaves a partial file or destroys the one it replaces.
pub fn encode_and_save_with_progress(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    settings: &ExportSettings,
    path: &Path,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let partial = partial_path(path);
    let written = encode(rgb_bytes, width, height, settings, &partial, on_progress)
        .and_then(|()| std::fs::rename(&partial, path).map_err(|e| e.to_string()));
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// A hidden sibling of `path`, on the same volume so the final rename is atomic.
fn partial_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    path.with_file_name(format!(".{name}.partial"))
}

fn encode(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    settings: &ExportSettings,
    path: &Path,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    // Built once here rather than per encoder: the profile describes the render,
    // whatever container it ends up in. Greyscale output, written with a single
    // sample per pixel, needs a profile naming a single channel.
    let greyscale = settings.greyscale();
    let icc = settings.output_space.icc(greyscale)?;

    match settings.format {
        ExportFormat::Png  => encode_png(&rgb_bytes, width, height, settings.png_compression, icc, path, on_progress),
        ExportFormat::Jpeg => encode_jpeg(rgb_bytes, width, height, settings.jpeg_quality, icc, path),
        ExportFormat::Webp => encode_webp(rgb_bytes, width, height, settings.webp_lossless, settings.webp_quality, settings.webp_compression, &icc, path),
        ExportFormat::Tiff => encode_tiff(rgb_bytes, width, height, settings.tiff_bit_depth, settings.is_bw, &icc, path, on_progress),
        ExportFormat::Jxl  => encode_jxl(rgb_bytes, width, height, &JxlOptions {
            lossless: settings.jxl_lossless,
            quality: settings.jxl_quality,
            effort: settings.jxl_effort,
            bit_depth: settings.jxl_depth(),
            greyscale,
        }, &icc, path),
    }
}

// ── PNG ───────────────────────────────────────────────────────────────────────

/// Rows handed to the PNG stream between progress reports.
const PNG_ROWS_PER_REPORT: usize = 64;

/// Written through a stream rather than in one call, so that progress can be
/// reported as the rows go out.
fn encode_png(
    rgb_bytes: &[u8],
    width: u32,
    height: u32,
    compression: u8,
    icc: Vec<u8>,
    path: &Path,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let row_bytes = width as usize * 3;
    if rgb_bytes.len() != row_bytes * height as usize {
        return Err(format!(
            "PNG input is {} bytes, expected {}",
            rgb_bytes.len(),
            row_bytes * height as usize
        ));
    }

    let mut info = png::Info::with_size(width, height);
    info.icc_profile = Some(Cow::Owned(icc));
    info.color_type = png::ColorType::Rgb;
    info.bit_depth = png::BitDepth::Eight;

    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder =
        png::Encoder::with_info(BufWriter::new(file), info).map_err(|e| e.to_string())?;
    encoder.set_compression(match compression {
        0..=2 => png::Compression::Fast,
        3..=6 => png::Compression::Balanced,
        _ => png::Compression::High,
    });
    encoder.set_filter(png::Filter::Adaptive);

    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;

    let mut written = 0usize;
    for rows in rgb_bytes.chunks(row_bytes * PNG_ROWS_PER_REPORT) {
        stream.write_all(rows).map_err(|e| e.to_string())?;
        written += rows.len();
        on_progress(written as f32 / rgb_bytes.len() as f32);
    }

    stream.finish().map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

// ── JPEG ──────────────────────────────────────────────────────────────────────

fn encode_jpeg(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    quality: u8,
    icc: Vec<u8>,
    path: &Path,
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = JpegEncoder::new_with_quality(BufWriter::new(file), quality);
    encoder.set_icc_profile(icc).map_err(|e| e.to_string())?;
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
    icc: &[u8],
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

    let tagged = webp_with_profile(&mem, width, height, icc)?;
    std::fs::write(path, &tagged).map_err(|e| e.to_string())
}

/// Bit 5 of the VP8X flag byte: this file carries an ICC profile.
const WEBP_ICC_FLAG: u8 = 0x20;

/// Rewrap a bare WebP bitstream in the extended format, carrying a profile.
///
/// libwebp writes the simple format — one `VP8 ` or `VP8L` chunk and nothing
/// else — which has nowhere to put a profile. The extended format does, at the
/// cost of a `VP8X` header declaring the canvas and what else is present. The
/// bitstream itself is copied through untouched.
fn webp_with_profile(
    webp: &[u8],
    width: u32,
    height: u32,
    icc: &[u8],
) -> Result<Vec<u8>, String> {
    const HEADER: usize = 12;

    if webp.len() < HEADER || &webp[0..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        return Err("WebP encoder produced something that is not a RIFF file".to_string());
    }
    // The canvas is stored minus one in 24 bits, so zero has no encoding and
    // 2^24 is one past the largest.
    if width == 0 || height == 0 || width > 1 << 24 || height > 1 << 24 {
        return Err(format!("WebP cannot describe a {width} × {height} canvas"));
    }

    let mut vp8x = [0u8; 10];
    vp8x[0] = WEBP_ICC_FLAG;
    vp8x[4..7].copy_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x[7..10].copy_from_slice(&(height - 1).to_le_bytes()[..3]);

    let mut out = Vec::with_capacity(webp.len() + icc.len() + 32);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&0u32.to_le_bytes()); // filled in below
    out.extend_from_slice(b"WEBP");
    riff_chunk(&mut out, b"VP8X", &vp8x);
    riff_chunk(&mut out, b"ICCP", icc);
    out.extend_from_slice(&webp[HEADER..]);

    let size = u32::try_from(out.len() - 8).map_err(|_| "WebP file is too large".to_string())?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// A RIFF chunk: tag, length, payload, and a pad byte to an even boundary that
/// the length does not count.
fn riff_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}

// ── TIFF ──────────────────────────────────────────────────────────────────────

/// The profile as TIFF's `InterColorProfile` tag wants it: bytes of undefined
/// type. A plain `[u8]` would be written as BYTE, which is a different tag type
/// from the one the specification names.
struct IccProfile<'a>(&'a [u8]);

impl TiffValue for IccProfile<'_> {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: Type = Type::UNDEFINED;

    fn count(&self) -> usize {
        self.0.len()
    }

    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.0)
    }
}

fn encode_tiff(
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
    bit_depth: u8,
    is_bw: bool,
    icc: &[u8],
    path: &Path,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder =
        TiffEncoder::new(BufWriter::new(file)).map_err(|e| e.to_string())?;

    // In BW mode R=G=B, so the greyscale paths take the red channel and ignore
    // the rest. render_image already produced u16 little-endian for 16-bit.
    let u16_at = |at: usize| u16::from_le_bytes([rgb_bytes[at], rgb_bytes[at + 1]]);

    match (bit_depth, is_bw) {
        (8, false) => write_tiff::<_, colortype::RGB8, _>(
            &mut encoder, width, height, icc, on_progress, |i| rgb_bytes[i],
        ),
        (8, true) => write_tiff::<_, colortype::Gray8, _>(
            &mut encoder, width, height, icc, on_progress, |i| rgb_bytes[i * 3],
        ),
        (16, false) => write_tiff::<_, colortype::RGB16, _>(
            &mut encoder, width, height, icc, on_progress, |i| u16_at(i * 2),
        ),
        (16, true) => write_tiff::<_, colortype::Gray16, _>(
            &mut encoder, width, height, icc, on_progress, |i| u16_at(i * 6),
        ),
        _ => Err(format!("Unsupported TIFF bit depth: {}", bit_depth)),
    }
}

/// Write one tagged image, a strip at a time, pulling samples on demand.
///
/// Strip-wise rather than in one call so that no path needs a second copy of
/// the image: the samples a strip wants are built as it is written, and a
/// 16-bit RGB export is already the largest buffer in the process.
fn write_tiff<W, C, F>(
    encoder: &mut TiffEncoder<W>,
    width: u32,
    height: u32,
    icc: &[u8],
    on_progress: &mut dyn FnMut(f32),
    sample: F,
) -> Result<(), String>
where
    W: Write + Seek,
    C: ColorType,
    [C::Inner]: TiffValue,
    F: Fn(usize) -> C::Inner,
{
    let mut image = encoder
        .new_image::<C>(width, height)
        .map_err(|e| e.to_string())?;

    // Tags belong to the directory, so this has to precede the image data.
    image
        .encoder()
        .write_tag(Tag::IccProfile, IccProfile(icc))
        .map_err(|e| e.to_string())?;

    let total = width as usize * height as usize * C::SAMPLE_FORMAT.len();
    let mut at = 0usize;
    let mut strip: Vec<C::Inner> = Vec::new();
    loop {
        let count = usize::try_from(image.next_strip_sample_count())
            .map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }

        strip.clear();
        strip.extend((at..at + count).map(&sample));
        image.write_strip(&strip).map_err(|e| e.to_string())?;
        at += count;
        on_progress(at as f32 / total as f32);
    }

    image.finish().map_err(|e| e.to_string())
}
