// src-tauri/tests/encoding.rs
//
// Layer 2: the last hop, from the renderer's raw RGB bytes to a file on disk.
// Lossless formats must round-trip exactly; lossy ones must stay close. This is
// also where the greyscale and 16-bit TIFF variants are exercised, since those
// reinterpret the buffer rather than just wrapping it.

mod common;

use std::path::PathBuf;

use image::GenericImageView;

use open_darkroom_lib::export_encoding::{encode_and_save, ExportFormat, ExportSettings};

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn settings(format: ExportFormat) -> ExportSettings {
    ExportSettings {
        format,
        png_compression: 6,
        jpeg_quality: 95,
        webp_lossless: true,
        webp_quality: 90,
        webp_compression: 6,
        tiff_bit_depth: 8,
        is_bw: false,
    }
}

/// A scratch path that is unique per test and cleaned up on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str, ext: &str) -> Self {
        let unique = format!(
            "open-darkroom-test-{}-{}-{}.{ext}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        Scratch(std::env::temp_dir().join(unique))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Deterministic 8-bit RGB test data, 3 bytes per pixel.
fn rgb8(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            out.push((x * 255 / width.max(1)) as u8);
            out.push((y * 255 / height.max(1)) as u8);
            out.push(((x + y) % 256) as u8);
        }
    }
    out
}

/// Deterministic 16-bit RGB test data, 6 bytes per pixel, little-endian.
fn rgb16(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 6) as usize);
    for y in 0..height {
        for x in 0..width {
            for c in 0..3u32 {
                let v = ((x * 7 + y * 13 + c * 1021) % 65536) as u16;
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    out
}

// ── Lossless formats ──────────────────────────────────────────────────────────

#[test]
fn png_round_trips_exactly() {
    let (w, h) = (37u32, 11);
    let source = rgb8(w, h);
    let scratch = Scratch::new("png", "png");

    encode_and_save(source.clone(), w, h, &settings(ExportFormat::Png), scratch.path())
        .expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));
    assert_eq!(decoded.to_rgb8().into_raw(), source, "PNG must be lossless");
}

#[test]
fn png_compression_levels_all_produce_valid_files() {
    let (w, h) = (16u32, 16);
    let source = rgb8(w, h);

    for level in [0u8, 3, 6, 9] {
        let scratch = Scratch::new(&format!("png{level}"), "png");
        let mut s = settings(ExportFormat::Png);
        s.png_compression = level;

        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");
        let decoded = image::open(scratch.path()).expect("decode");

        assert_eq!(
            decoded.to_rgb8().into_raw(),
            source,
            "compression {level} changed the pixels"
        );
    }
}

#[test]
fn tiff_round_trips_exactly_at_eight_bits() {
    let (w, h) = (23u32, 9);
    let source = rgb8(w, h);
    let scratch = Scratch::new("tiff8", "tiff");

    encode_and_save(source.clone(), w, h, &settings(ExportFormat::Tiff), scratch.path())
        .expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));
    assert_eq!(decoded.to_rgb8().into_raw(), source);
}

#[test]
fn tiff_round_trips_exactly_at_sixteen_bits() {
    let (w, h) = (15u32, 7);
    let source = rgb16(w, h);
    let scratch = Scratch::new("tiff16", "tiff");

    let mut s = settings(ExportFormat::Tiff);
    s.tiff_bit_depth = 16;

    encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));

    let round_tripped: Vec<u8> = decoded
        .to_rgb16()
        .into_raw()
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(round_tripped, source, "16-bit TIFF must preserve every level");
}

#[test]
fn webp_lossless_round_trips_exactly() {
    let (w, h) = (20u32, 12);
    let source = rgb8(w, h);
    let scratch = Scratch::new("webp", "webp");

    encode_and_save(source.clone(), w, h, &settings(ExportFormat::Webp), scratch.path())
        .expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));
    assert_eq!(
        decoded.to_rgb8().into_raw(),
        source,
        "lossless WebP must be lossless"
    );
}

// ── Greyscale variants ────────────────────────────────────────────────────────

#[test]
fn bw_tiff_writes_a_single_channel_from_red() {
    let (w, h) = (8u32, 4);
    // R = G = B, as the BW mode guarantees.
    let source: Vec<u8> = (0..(w * h)).flat_map(|i| [i as u8, i as u8, i as u8]).collect();
    let scratch = Scratch::new("tiffbw", "tiff");

    let mut s = settings(ExportFormat::Tiff);
    s.is_bw = true;

    encode_and_save(source, w, h, &s, scratch.path()).expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));
    assert_eq!(
        decoded.color(),
        image::ColorType::L8,
        "BW mode should write a greyscale TIFF, not RGB"
    );

    let grey = decoded.to_luma8().into_raw();
    for (i, &v) in grey.iter().enumerate() {
        assert_eq!(v, i as u8, "grey level {i}");
    }
}

#[test]
fn bw_tiff_at_sixteen_bits_keeps_the_full_range() {
    let (w, h) = (4u32, 2);
    let values: Vec<u16> = (0..(w * h)).map(|i| (i as u16 + 1) * 8000).collect();
    let source: Vec<u8> = values
        .iter()
        .flat_map(|&v| [v, v, v])
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let scratch = Scratch::new("tiffbw16", "tiff");

    let mut s = settings(ExportFormat::Tiff);
    s.is_bw = true;
    s.tiff_bit_depth = 16;

    encode_and_save(source, w, h, &s, scratch.path()).expect("encode");

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.color(), image::ColorType::L16);
    assert_eq!(decoded.to_luma16().into_raw(), values);
}

// ── Lossy formats ─────────────────────────────────────────────────────────────

#[test]
fn jpeg_stays_close_to_the_source() {
    let (w, h) = (32u32, 32);
    let source = rgb8(w, h);
    let scratch = Scratch::new("jpeg", "jpg");

    encode_and_save(source.clone(), w, h, &settings(ExportFormat::Jpeg), scratch.path())
        .expect("encode");

    let decoded = image::open(scratch.path()).expect("decode").to_rgb8().into_raw();
    assert_eq!(decoded.len(), source.len());

    let mean_error: f64 = decoded
        .iter()
        .zip(source.iter())
        .map(|(&a, &b)| (a as f64 - b as f64).abs())
        .sum::<f64>()
        / source.len() as f64;

    assert!(
        mean_error < 4.0,
        "quality 95 JPEG drifted by {mean_error:.2} on average"
    );
}

#[test]
fn higher_jpeg_quality_produces_a_larger_file() {
    let (w, h) = (64u32, 64);
    let source = rgb8(w, h);

    let size_at = |quality: u8| {
        let scratch = Scratch::new(&format!("jpegq{quality}"), "jpg");
        let mut s = settings(ExportFormat::Jpeg);
        s.jpeg_quality = quality;
        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");
        std::fs::metadata(scratch.path()).expect("stat").len()
    };

    assert!(
        size_at(95) > size_at(30),
        "the quality slider is not reaching the encoder"
    );
}

#[test]
fn the_webp_lossless_flag_reaches_the_encoder() {
    // Compare by fidelity rather than file size: on a smooth synthetic gradient
    // WebP's lossless mode can beat lossy on size, so size proves nothing.
    // Noise is incompressible, which makes the distinction unambiguous.
    let (w, h) = (64u32, 64);
    let source: Vec<u8> = {
        let mut state = 0x1234_5678u32;
        (0..(w * h * 3))
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect()
    };

    let decode_with = |lossless: bool| {
        let scratch = Scratch::new(&format!("webp{lossless}"), "webp");
        let mut s = settings(ExportFormat::Webp);
        s.webp_lossless = lossless;
        s.webp_quality = 60;
        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");
        image::open(scratch.path())
            .expect("decode")
            .to_rgb8()
            .into_raw()
    };

    assert_eq!(decode_with(true), source, "lossless mode must be exact");
    assert_ne!(
        decode_with(false),
        source,
        "lossy mode returned the source untouched — the flag is not reaching the encoder"
    );
}

// ── Failure modes ─────────────────────────────────────────────────────────────

#[test]
fn an_unsupported_tiff_bit_depth_is_reported() {
    let scratch = Scratch::new("tiffbad", "tiff");
    let mut s = settings(ExportFormat::Tiff);
    s.tiff_bit_depth = 12;

    let err = encode_and_save(rgb8(4, 4), 4, 4, &s, scratch.path())
        .expect_err("12-bit TIFF is not supported");
    assert!(err.contains("12"), "error should name the bit depth: {err}");
}
