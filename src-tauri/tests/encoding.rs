// src-tauri/tests/encoding.rs
//
// Layer 2: the last hop, from the renderer's raw RGB bytes to a file on disk.
// Lossless formats must round-trip exactly; lossy ones must stay close. This is
// also where the greyscale and 16-bit TIFF variants are exercised, since those
// reinterpret the buffer rather than just wrapping it.

mod common;

use std::path::PathBuf;

use image::GenericImageView;

use open_darkroom_lib::export_encoding::{
    encode_and_save, encode_and_save_with_progress, ExportFormat, ExportSettings,
};
use open_darkroom_lib::icc::SourceSpace;
use open_darkroom_lib::output_space::OutputSpace;

// ── Fixtures ──────────────────────────────────────────────────────────────────

fn settings(format: ExportFormat) -> ExportSettings {
    ExportSettings {
        format,
        output_space: OutputSpace::Srgb,
        png_compression: 6,
        jpeg_quality: 95,
        webp_lossless: true,
        webp_quality: 90,
        webp_compression: 6,
        tiff_bit_depth: 8,
        jxl_lossless: true,
        jxl_quality: 90,
        jxl_effort: 7,
        jxl_bit_depth: 8,
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

// ── JPEG XL ───────────────────────────────────────────────────────────────────

/// Decode a JPEG XL file to its samples, rescaled to the integer range it was
/// written at, along with the number of colour channels.
fn decode_jxl(path: &std::path::Path, max: f32) -> (u32, u32, usize, Vec<u16>) {
    let image = jxl_oxide::JxlImage::builder().open(path).expect("decode JPEG XL");
    let render = image.render_frame(0).expect("render JPEG XL");
    let frame = render.image_all_channels();
    let samples = frame
        .buf()
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * max).round() as u16)
        .collect();
    (image.width(), image.height(), frame.channels(), samples)
}

/// Film grain stand-in: a smooth ramp under luminance noise that clumps over a
/// few pixels and is shared by all three channels, as dye clouds mostly are.
fn grainy16(width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut state = 0x2545_f491_u32;
    let white: Vec<f64> = (0..w * h)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            f64::from(state % 10_000) / 10_000.0 - 0.5
        })
        .collect();

    let grain = |x: usize, y: usize| {
        let (mut sum, mut weight) = (0.0, 0.0);
        for (dx, dy) in (-1i64..=1).flat_map(|dy| (-1i64..=1).map(move |dx| (dx, dy))) {
            let (xx, yy) = (x as i64 + dx, y as i64 + dy);
            if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                let k = if dx == 0 && dy == 0 { 2.0 } else { 1.0 };
                sum += k * white[yy as usize * w + xx as usize];
                weight += k;
            }
        }
        sum / weight
    };

    let mut out = Vec::with_capacity(w * h * 6);
    for y in 0..h {
        for x in 0..w {
            let g = grain(x, y);
            for c in 0..3 {
                let base = 15_000.0 + (x * 30_000 / w + y * 10_000 / h + c * 3_000) as f64;
                let v = base + 6_000.0 * g * (1.0 + 0.05 * c as f64);
                out.extend_from_slice(&(v.clamp(0.0, 65535.0) as u16).to_le_bytes());
            }
        }
    }
    out
}

/// Each sample minus the mean of its horizontal neighbours on the same channel:
/// what is left is the grain, with the image underneath removed.
fn high_pass(samples: &[f64], width: usize) -> Vec<f64> {
    (0..samples.len())
        .map(|i| {
            let x = (i / 3) % width;
            if x < 3 || x + 3 >= width {
                return 0.0;
            }
            let mean = (-3i64..=3)
                .map(|d| samples[(i as i64 + d * 3) as usize])
                .sum::<f64>()
                / 7.0;
            samples[i] - mean
        })
        .collect()
}

#[test]
fn jxl_lossless_round_trips_exactly_at_eight_bits() {
    let (w, h) = (37u32, 11);
    let source = rgb8(w, h);
    let scratch = Scratch::new("jxl8", "jxl");

    encode_and_save(source.clone(), w, h, &settings(ExportFormat::Jxl), scratch.path())
        .expect("encode");

    let (dw, dh, channels, decoded) = decode_jxl(scratch.path(), 255.0);
    assert_eq!((dw, dh, channels), (w, h, 3));
    let decoded: Vec<u8> = decoded.iter().map(|&v| v as u8).collect();
    assert_eq!(decoded, source, "lossless JPEG XL must be exact");
}

#[test]
fn jxl_lossless_round_trips_exactly_at_sixteen_bits() {
    let (w, h) = (23u32, 9);
    let source = grainy16(w, h);
    let scratch = Scratch::new("jxl16", "jxl");

    let mut s = settings(ExportFormat::Jxl);
    s.jxl_bit_depth = 16;
    s.output_space = OutputSpace::Rec2020;
    encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

    let (_, _, _, decoded) = decode_jxl(scratch.path(), 65535.0);
    let decoded: Vec<u8> = decoded.iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(decoded, source, "16-bit lossless JPEG XL must preserve every level");
}

#[test]
fn bw_jxl_writes_a_single_channel_tagged_grey() {
    let (w, h) = (8u32, 4);
    let values: Vec<u16> = (0..(w * h)).map(|i| (i as u16 + 1) * 2000).collect();
    let source: Vec<u8> = values
        .iter()
        .flat_map(|&v| [v, v, v])
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let scratch = Scratch::new("jxlbw", "jxl");

    let mut s = settings(ExportFormat::Jxl);
    s.is_bw = true;
    s.jxl_bit_depth = 16;
    encode_and_save(source, w, h, &s, scratch.path()).expect("encode");

    let (_, _, channels, decoded) = decode_jxl(scratch.path(), 65535.0);
    assert_eq!(channels, 1, "BW mode should write a greyscale JPEG XL");
    assert_eq!(decoded, values);

    let icc = embedded_profile(scratch.path()).expect("a greyscale JPEG XL carries no profile");
    let profile = lcms2::Profile::new_icc(&icc).expect("parse the embedded profile");
    assert_eq!(profile.color_space(), lcms2::ColorSpaceSignature::GrayData);
}

/// Lossy coding is free to swap the grain for something statistically similar
/// and still score well on error. What matters here is that the grain coming
/// back is the same grain, so it is compared pattern against pattern.
#[test]
fn jxl_lossy_keeps_the_grain_and_is_smaller_than_lossless() {
    let (w, h) = (256u32, 192);
    let source = grainy16(w, h);

    let encode = |lossless: bool, quality: u8| {
        let scratch = Scratch::new(&format!("jxl-{lossless}-{quality}"), "jxl");
        let mut s = settings(ExportFormat::Jxl);
        s.jxl_bit_depth = 16;
        s.jxl_lossless = lossless;
        s.jxl_quality = quality;
        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");
        let size = std::fs::metadata(scratch.path()).expect("stat").len();
        let samples: Vec<f64> = decode_jxl(scratch.path(), 65535.0)
            .3
            .iter()
            .map(|&v| f64::from(v))
            .collect();
        (size, samples)
    };

    let (lossless_size, _) = encode(true, 90);
    let (high_size, high) = encode(false, 95);
    let (low_size, _) = encode(false, 60);

    assert!(high_size < lossless_size, "lossy {high_size} B is not smaller than lossless {lossless_size} B");
    assert!(low_size < high_size, "quality 60 ({low_size} B) is not smaller than 95 ({high_size} B)");

    let source: Vec<f64> = source
        .chunks_exact(2)
        .map(|c| f64::from(u16::from_le_bytes([c[0], c[1]])))
        .collect();
    let (a, b) = (high_pass(&source, w as usize), high_pass(&high, w as usize));
    let dot = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(x, y)| x * y).sum::<f64>();
    let correlation = dot(&a, &b) / (dot(&a, &a) * dot(&b, &b)).sqrt();
    assert!(
        correlation > 0.85,
        "quality 95 kept only {correlation:.2} of the grain pattern"
    );
}

/// Depths between 8 and 16 take 16-bit input and store it requantised, so the
/// file must hold exactly the input rounded to that many bits.
#[test]
fn jxl_lossless_at_twelve_bits_keeps_twelve_bits() {
    let (w, h) = (23u32, 9);
    let source = grainy16(w, h);
    let scratch = Scratch::new("jxl12", "jxl");

    let mut s = settings(ExportFormat::Jxl);
    s.jxl_bit_depth = 12;
    assert_eq!(s.bit_depth(), 16, "a 12-bit JPEG XL is rendered at 16");
    encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

    let image = jxl_oxide::JxlImage::builder().open(scratch.path()).expect("decode");
    assert_eq!(image.image_header().metadata.bit_depth.bits_per_sample(), 12);

    let (_, _, _, decoded) = decode_jxl(scratch.path(), 4095.0);
    let worst = source
        .chunks_exact(2)
        .map(|c| (f64::from(u16::from_le_bytes([c[0], c[1]])) * 4095.0 / 65535.0).round() as i32)
        .zip(&decoded)
        .map(|(expected, &got)| (expected - i32::from(got)).abs())
        .max()
        .unwrap();
    assert!(worst <= 1, "12-bit samples drifted by up to {worst} codes");
}

/// Past 2048 pixels libjxl pulls the input in rectangles and may encode it in
/// independent groups; neither may change a lossless result.
#[test]
fn a_large_jxl_round_trips_exactly_when_read_in_pieces() {
    let (w, h) = (2100u32, 1100);
    let source = grainy16(w, h);

    for is_bw in [false, true] {
        let scratch = Scratch::new(&format!("jxl-large-{is_bw}"), "jxl");
        let mut s = settings(ExportFormat::Jxl);
        s.jxl_bit_depth = 16;
        s.jxl_effort = 3;
        s.is_bw = is_bw;
        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

        let expected: Vec<u16> = source
            .chunks_exact(if is_bw { 6 } else { 2 })
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let (dw, dh, channels, decoded) = decode_jxl(scratch.path(), 65535.0);
        assert_eq!((dw, dh, channels), (w, h, if is_bw { 1 } else { 3 }));
        assert!(decoded == expected, "large lossless JPEG XL (BW: {is_bw}) is not exact");
    }
}

#[test]
fn an_unsupported_jxl_bit_depth_is_reported() {
    let scratch = Scratch::new("jxlbad", "jxl");
    let mut s = settings(ExportFormat::Jxl);
    s.jxl_bit_depth = 17;

    let err = encode_and_save(rgb8(4, 4), 4, 4, &s, scratch.path())
        .expect_err("17-bit JPEG XL is not supported");
    assert!(err.contains("17"), "error should name the bit depth: {err}");
}

// ── Encode progress ───────────────────────────────────────────────────────────

/// The formats that report progress must climb to completion, so the bar's
/// encoding phase ends where the file does.
#[test]
fn png_and_tiff_report_progress_up_to_completion() {
    // Large enough for TIFF to need several strips.
    let (w, h) = (600u32, 1000);

    for (format, ext) in [(ExportFormat::Png, "png"), (ExportFormat::Tiff, "tiff")] {
        let scratch = Scratch::new(&format!("progress-{ext}"), ext);
        let mut seen = Vec::new();
        encode_and_save_with_progress(
            rgb8(w, h),
            w,
            h,
            &settings(format),
            None,
            scratch.path(),
            &mut |p| seen.push(p),
        )
        .expect("encode");

        assert!(seen.len() > 1, "{format:?} reported {seen:?}");
        assert!(seen.windows(2).all(|p| p[1] >= p[0]), "{format:?} went backwards: {seen:?}");
        assert_eq!(*seen.last().unwrap(), 1.0, "{format:?} did not finish at 100%");
        assert_eq!(
            image::open(scratch.path()).expect("decode").to_rgb8().into_raw(),
            rgb8(w, h),
            "{format:?} must stay lossless when written in pieces"
        );
    }
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

fn partial_file_of(path: &std::path::Path) -> PathBuf {
    let name = path.file_name().unwrap().to_string_lossy();
    path.with_file_name(format!(".{name}.partial"))
}

#[test]
fn a_failed_export_leaves_the_file_it_would_replace_untouched() {
    for (format, ext) in [(ExportFormat::Png, "png"), (ExportFormat::Jxl, "jxl")] {
        let scratch = Scratch::new(&format!("keep-{ext}"), ext);
        std::fs::write(scratch.path(), b"earlier export").unwrap();

        // One byte short of a 4 × 4 image.
        let mut short = rgb8(4, 4);
        short.pop();
        encode_and_save(short, 4, 4, &settings(format), scratch.path())
            .expect_err("a short buffer must fail");

        assert_eq!(std::fs::read(scratch.path()).unwrap(), b"earlier export", "{format:?}");
        assert!(!partial_file_of(scratch.path()).exists(), "{format:?} left a partial file");
    }
}

#[test]
fn a_successful_export_replaces_the_file_and_cleans_up() {
    let scratch = Scratch::new("replace", "png");
    std::fs::write(scratch.path(), b"earlier export").unwrap();

    encode_and_save(rgb8(4, 4), 4, 4, &settings(ExportFormat::Png), scratch.path())
        .expect("encode");

    assert_eq!(image::open(scratch.path()).expect("decode").to_rgb8().into_raw(), rgb8(4, 4));
    assert!(!partial_file_of(scratch.path()).exists());
}

// ── The embedded profile ──────────────────────────────────────────────────────

/// TIFF's `InterColorProfile`, as the specification numbers it, and the type it
/// is defined as: TIFF type 7, bytes whose meaning the tag itself gives.
const TIFF_ICC_TAG: u16 = 34675;
const TIFF_TYPE_UNDEFINED: u16 = 7;

/// A TIFF's first-IFD entry for `tag`: its type, and its value as bytes.
///
/// Read by hand rather than through a library, because no library here can
/// vouch for it. `image`'s TIFF decoder looks the profile up as
/// `Tag::Unknown(34675)` while the `tiff` crate parses that same id into its
/// named `IccProfile` variant, so the two never meet and a profile that is
/// present reads as absent. Whether the bytes are right is a question about the
/// file, so it is answered from the file.
fn tiff_entry(bytes: &[u8], tag: u16) -> Option<(u16, Vec<u8>)> {
    let little = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        other => panic!("not a TIFF byte order mark: {other:?}"),
    };
    let short = |at: usize| {
        let b = [bytes[at], bytes[at + 1]];
        if little { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }
    };
    let long = |at: usize| {
        let b = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
        if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
    };

    assert_eq!(short(2), 42, "not a classic TIFF");
    let ifd = long(4) as usize;

    (0..short(ifd) as usize).find_map(|i| {
        // Each entry is 12 bytes: tag, type, count, then the value or its offset.
        let entry = ifd + 2 + i * 12;
        if short(entry) != tag {
            return None;
        }
        let count = long(entry + 4) as usize;
        // A value longer than four bytes lives elsewhere, which a profile always
        // does, so the last field is its offset rather than the value.
        let at = if count > 4 { long(entry + 8) as usize } else { entry + 8 };
        Some((short(entry + 2), bytes[at..at + count].to_vec()))
    })
}

/// Pull the ICC profile back out of a written file.
///
/// TIFF is read by hand, for the reason above; the other three containers are
/// read back through `image`, which is also what put the profile there.
fn embedded_profile(path: &std::path::Path) -> Option<Vec<u8>> {
    use image::ImageDecoder;

    if path.extension().is_some_and(|e| e == "jxl") {
        let image = jxl_oxide::JxlImage::builder().open(path).expect("decode JPEG XL");
        return image.original_icc().map(<[u8]>::to_vec);
    }

    if path.extension().is_some_and(|e| e == "tiff" || e == "tif") {
        let bytes = std::fs::read(path).expect("read");
        let (field_type, profile) = tiff_entry(&bytes, TIFF_ICC_TAG)?;
        assert_eq!(
            field_type, TIFF_TYPE_UNDEFINED,
            "the profile is tagged as type {field_type}, not UNDEFINED — a reader \
             that checks the type will skip it"
        );
        return Some(profile);
    }

    image::ImageReader::open(path)
        .expect("open")
        .with_guessed_format()
        .expect("format")
        .into_decoder()
        .expect("decoder")
        .icc_profile()
        .expect("read the profile")
}

/// Every format carries the profile of the space it was rendered in.
///
/// Untagged 8-bit sRGB was right by accident — it is what most software assumes.
/// Nothing would have rescued an untagged Adobe RGB or Rec.2020 file: it opens
/// as sRGB, so the colour is wrong by exactly the gamut difference, and there is
/// no way to tell from the file that it was ever anything else.
#[test]
fn every_format_carries_the_profile_of_its_space() {
    let (w, h) = (12u32, 8);

    for format in [
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::Webp,
        ExportFormat::Tiff,
        ExportFormat::Jxl,
    ] {
        for space in OutputSpace::ALL {
            let label = format!("{format:?}/{space:?}");
            let ext = match format {
                ExportFormat::Png => "png",
                ExportFormat::Jpeg => "jpg",
                ExportFormat::Webp => "webp",
                ExportFormat::Tiff => "tiff",
                ExportFormat::Jxl => "jxl",
            };
            let scratch = Scratch::new(&label.replace('/', "-").to_lowercase(), ext);

            let mut s = settings(format);
            s.output_space = space;

            encode_and_save(rgb8(w, h), w, h, &s, scratch.path())
                .unwrap_or_else(|e| panic!("{label}: encode failed: {e}"));

            let icc = embedded_profile(scratch.path())
                .unwrap_or_else(|| panic!("{label}: the file carries no profile"));

            // Read back through the app's own input path: whatever the container
            // did to the bytes, the profile still has to mean the space the
            // render was converted into.
            let resolved = SourceSpace::resolve(Some(icc));
            let expected = space.destination_to_working();
            for i in 0..9 {
                assert!(
                    (resolved.to_working[i] - expected[i]).abs() < 2e-3,
                    "{label}: the embedded profile describes a different space \
                     (element {i}: {} vs {})",
                    resolved.to_working[i],
                    expected[i]
                );
            }
        }
    }
}

/// A 16-bit TIFF is the case the tag matters most for, and the one where it is
/// written through a strip-wise path rather than in one call.
#[test]
fn a_sixteen_bit_tiff_is_tagged_and_still_exact() {
    let (w, h) = (23u32, 9);
    let source = rgb16(w, h);
    let scratch = Scratch::new("tiff16-tagged", "tiff");

    let mut s = settings(ExportFormat::Tiff);
    s.tiff_bit_depth = 16;
    s.output_space = OutputSpace::Rec2020;
    encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

    // Nothing assumes sRGB at 16 bits, so an untagged archival file is one whose
    // colour nobody can reconstruct.
    assert!(
        embedded_profile(scratch.path()).is_some_and(|p| !p.is_empty()),
        "a 16-bit TIFF must carry its profile"
    );

    let decoded = image::open(scratch.path()).expect("decode");
    assert_eq!(decoded.dimensions(), (w, h));
    assert_eq!(
        decoded.to_rgb16().into_raw(),
        source
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect::<Vec<u16>>(),
        "writing the profile must not disturb the samples"
    );
}

/// A greyscale TIFF takes a different code path for its samples, and the tag
/// has to survive that one too.
#[test]
fn a_greyscale_tiff_is_tagged() {
    let (w, h) = (16u32, 6);

    for bit_depth in [8u8, 16] {
        let scratch = Scratch::new(&format!("tiff-grey{bit_depth}"), "tiff");
        let mut s = settings(ExportFormat::Tiff);
        s.tiff_bit_depth = bit_depth;
        s.is_bw = true;

        let source = if bit_depth == 8 { rgb8(w, h) } else { rgb16(w, h) };
        encode_and_save(source, w, h, &s, scratch.path()).expect("encode");

        let icc = embedded_profile(scratch.path())
            .unwrap_or_else(|| panic!("a {bit_depth}-bit greyscale TIFF carries no profile"));

        // A profile has to describe the samples the file actually holds. An RGB
        // profile on a one-sample-per-pixel image is a mismatch a colour-managed
        // reader is entitled to reject, so presence alone is not the assertion.
        let profile = lcms2::Profile::new_icc(&icc).expect("parse the embedded profile");
        assert_eq!(
            profile.color_space(),
            lcms2::ColorSpaceSignature::GrayData,
            "a {bit_depth}-bit greyscale TIFF is tagged {:?}, but carries one sample per pixel",
            profile.color_space()
        );
        assert_eq!(
            image::open(scratch.path()).expect("decode").dimensions(),
            (w, h)
        );
    }
}

/// libwebp writes the simple container, which has nowhere to put a profile, so
/// the file is rewrapped in the extended one. That rewrap moves the bitstream
/// without touching it, and getting it wrong would produce a file that either
/// fails to decode or decodes to different pixels.
#[test]
fn rewrapping_webp_for_its_profile_preserves_the_image() {
    let (w, h) = (29u32, 13);
    let source = rgb8(w, h);

    for lossless in [true, false] {
        let scratch = Scratch::new(&format!("webp-icc-{lossless}"), "webp");
        let mut s = settings(ExportFormat::Webp);
        s.webp_lossless = lossless;
        encode_and_save(source.clone(), w, h, &s, scratch.path()).expect("encode");

        let bytes = std::fs::read(scratch.path()).expect("read");
        assert_eq!(&bytes[0..4], b"RIFF", "lossless={lossless}: not a RIFF file");
        assert_eq!(&bytes[8..12], b"WEBP", "lossless={lossless}: not a WebP file");
        assert_eq!(
            &bytes[12..16],
            b"VP8X",
            "lossless={lossless}: the extended header is missing, so the \
             profile has nowhere to live"
        );
        // The RIFF size field counts everything after it.
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize,
            bytes.len() - 8,
            "lossless={lossless}: the RIFF length does not match the file"
        );

        let decoded = image::open(scratch.path()).expect("decode");
        assert_eq!(
            decoded.dimensions(),
            (w, h),
            "lossless={lossless}: the canvas in the extended header is wrong"
        );
        if lossless {
            assert_eq!(
                decoded.to_rgb8().into_raw(),
                source,
                "lossless WebP must stay lossless through the rewrap"
            );
        }
        assert!(
            embedded_profile(scratch.path()).is_some(),
            "lossless={lossless}: the rewrapped file carries no profile"
        );
    }
}
