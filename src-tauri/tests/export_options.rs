// src-tauri/tests/export_options.rs
//
// Layer 1: what the export modal adds around the render. The shooting
// information has to come back out of every format it is written into, the
// size has to land where the modal says it will, and the file name has to be
// one every platform can hold.

use std::path::{Path, PathBuf};

use open_darkroom_lib::exif_writing::{exif_block, exif_tags, tag};
use open_darkroom_lib::export::export_file_name;
use open_darkroom_lib::export_encoding::{encode_and_save_with_progress, ExportFormat, ExportSettings};
use open_darkroom_lib::export_size::{resize_rgb, target_size, OutputSize, ResizeMode};
use open_darkroom_lib::metadata::{from_exif_block, ImageMetadata};
use open_darkroom_lib::output_space::{OutputSpace, Transfer};

fn shot() -> ImageMetadata {
    ImageMetadata {
        file_name: String::new(),
        make: Some("Hasselblad".into()),
        model: Some("Hasselblad 500C/M".into()),
        lens: Some("Carl Zeiss Planar 80mm f/2.8".into()),
        focal_length: Some(80.0),
        aperture: Some(2.8),
        exposure_time: Some([1, 250]),
        iso: Some(400),
        exposure_bias: Some(-1.0 / 3.0),
        date_taken: Some("2024:06:01 14:30:00".into()),
    }
}

fn assert_same_shot(read: &ImageMetadata) {
    let expected = shot();
    assert_eq!(read.make, expected.make);
    assert_eq!(read.model, expected.model);
    assert_eq!(read.lens, expected.lens);
    assert_eq!(read.focal_length, expected.focal_length);
    assert_eq!(read.aperture, expected.aperture);
    assert_eq!(read.exposure_time, expected.exposure_time);
    assert_eq!(read.iso, expected.iso);
    assert!((read.exposure_bias.unwrap() - expected.exposure_bias.unwrap()).abs() < 1e-9);
    assert_eq!(read.date_taken, expected.date_taken);
}

// ── EXIF ──────────────────────────────────────────────────────────────────────

#[test]
fn the_exif_block_reads_back_as_what_it_was_built_from() {
    let block = exif_block(&exif_tags(&shot()));
    assert_same_shot(&from_exif_block(&block).expect("parse"));
}

#[test]
fn an_unknown_shot_still_records_the_orientation_and_no_exif_directory() {
    let tags = exif_tags(&ImageMetadata::default());
    assert!(tags.exif.is_empty());
    assert!(tags.primary.iter().any(|(t, _)| *t == tag::ORIENTATION));
    assert!(from_exif_block(&exif_block(&tags)).is_some());
}

fn settings(format: ExportFormat) -> ExportSettings {
    ExportSettings {
        format,
        output_space: OutputSpace::Srgb,
        png_compression: 6,
        jpeg_quality: 90,
        webp_lossless: true,
        webp_quality: 90,
        webp_compression: 6,
        tiff_bit_depth: 8,
        jxl_lossless: false,
        jxl_quality: 90,
        jxl_effort: 3,
        jxl_bit_depth: 8,
        is_bw: false,
    }
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(ext: &str) -> Self {
        let unique = format!(
            "open-darkroom-options-{}-{}.{ext}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        Scratch(std::env::temp_dir().join(unique))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn encode(format: ExportFormat, bit_depth: u8, with_metadata: bool) -> (Scratch, Vec<u8>) {
    let (w, h) = (24u32, 16);
    let mut s = settings(format);
    s.tiff_bit_depth = bit_depth;
    let bytes = if s.bit_depth() > 8 { 6 } else { 3 };
    let rgb: Vec<u8> = (0..w * h * bytes).map(|i| (i * 7 % 251) as u8).collect();
    let scratch = Scratch::new(format.extension());
    let tags = exif_tags(&shot());
    encode_and_save_with_progress(rgb, w, h, &s, with_metadata.then_some(&tags), scratch.path(), &mut |_| {})
        .expect("encode");
    let file = std::fs::read(scratch.path()).expect("read");
    (scratch, file)
}

/// The payload of the first top-level ISOBMFF box of this type.
fn jxl_box<'a>(file: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 0;
    while at + 8 <= file.len() {
        let size = u32::from_be_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        let size = if size == 0 { file.len() - at } else { size };
        if &file[at + 4..at + 8] == kind {
            return Some(&file[at + 8..at + size]);
        }
        at += size.max(8);
    }
    None
}

/// The payload of the first RIFF chunk of this type.
fn webp_chunk<'a>(file: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 12;
    while at + 8 <= file.len() {
        let size = u32::from_le_bytes(file[at + 4..at + 8].try_into().unwrap()) as usize;
        if &file[at..at + 4] == kind {
            return Some(&file[at + 8..at + 8 + size]);
        }
        at += 8 + size + size % 2;
    }
    None
}

#[test]
fn jpeg_and_png_carry_the_shot_where_an_open_reads_it() {
    for format in [ExportFormat::Jpeg, ExportFormat::Png] {
        let (scratch, _) = encode(format, 8, true);
        assert_same_shot(&open_darkroom_lib::metadata::read(scratch.path()));
        image::open(scratch.path()).expect("still decodes");
    }
}

#[test]
fn webp_carries_the_shot_in_an_exif_chunk_and_says_so() {
    let (scratch, file) = encode(ExportFormat::Webp, 8, true);
    let vp8x = webp_chunk(&file, b"VP8X").expect("extended format");
    assert_ne!(vp8x[0] & 0x08, 0, "the EXIF flag must be set");
    assert!(webp_chunk(&file, b"ICCP").is_some());
    assert_same_shot(&from_exif_block(webp_chunk(&file, b"EXIF").expect("EXIF chunk")).unwrap());
    image::open(scratch.path()).expect("still decodes");
}

#[test]
fn tiff_carries_the_shot_in_its_own_directories() {
    for depth in [8, 16] {
        let (scratch, file) = encode(ExportFormat::Tiff, depth, true);
        // A TIFF is the structure an EXIF block borrows, so it parses as one.
        assert_same_shot(&from_exif_block(&file).expect("parse"));
        image::open(scratch.path()).expect("still decodes");
    }
}

#[test]
fn jxl_carries_the_shot_in_an_exif_box() {
    let (scratch, file) = encode(ExportFormat::Jxl, 8, true);
    let contents = jxl_box(&file, b"Exif").expect("Exif box");
    assert_eq!(&contents[..4], &[0, 0, 0, 0], "the TIFF header follows at once");
    assert_same_shot(&from_exif_block(&contents[4..]).unwrap());
    let image = jxl_oxide::JxlImage::builder().open(scratch.path()).expect("still decodes");
    image.render_frame(0).expect("still renders");
}

#[test]
fn without_metadata_no_format_carries_any() {
    for format in [ExportFormat::Jpeg, ExportFormat::Png, ExportFormat::Webp, ExportFormat::Tiff, ExportFormat::Jxl] {
        let (scratch, file) = encode(format, 8, false);
        let read = open_darkroom_lib::metadata::read(scratch.path());
        assert_eq!(read.model, None, "{format:?}");
        match format {
            ExportFormat::Webp => assert!(webp_chunk(&file, b"EXIF").is_none()),
            ExportFormat::Jxl => assert!(jxl_box(&file, b"Exif").is_none()),
            ExportFormat::Tiff => assert_eq!(from_exif_block(&file).and_then(|m| m.model), None),
            _ => {}
        }
    }
}

// ── Size ──────────────────────────────────────────────────────────────────────

fn size(mode: ResizeMode, pixels: u32, percent: f64) -> OutputSize {
    OutputSize { mode, pixels, percent }
}

#[test]
fn each_mode_lands_its_edge_on_the_number_asked_for() {
    let (w, h) = (6000, 4000);
    assert_eq!(target_size(&size(ResizeMode::None, 100, 10.0), w, h), (6000, 4000));
    assert_eq!(target_size(&size(ResizeMode::LongEdge, 2048, 0.0), w, h), (2048, 1365));
    assert_eq!(target_size(&size(ResizeMode::LongEdge, 2048, 0.0), h, w), (1365, 2048));
    assert_eq!(target_size(&size(ResizeMode::ShortEdge, 1080, 0.0), w, h), (1620, 1080));
    assert_eq!(target_size(&size(ResizeMode::Width, 1200, 0.0), w, h), (1200, 800));
    assert_eq!(target_size(&size(ResizeMode::Height, 1000, 0.0), w, h), (1500, 1000));
    assert_eq!(target_size(&size(ResizeMode::Percent, 0, 25.0), w, h), (1500, 1000));
}

#[test]
fn an_export_is_never_enlarged() {
    let (w, h) = (800, 600);
    assert_eq!(target_size(&size(ResizeMode::LongEdge, 4000, 0.0), w, h), (800, 600));
    assert_eq!(target_size(&size(ResizeMode::Percent, 0, 150.0), w, h), (800, 600));
    assert_eq!(target_size(&size(ResizeMode::Percent, 0, f64::NAN), w, h), (800, 600));
}

#[test]
fn a_thin_strip_keeps_at_least_a_pixel() {
    assert_eq!(target_size(&size(ResizeMode::LongEdge, 10, 0.0), 10_000, 3), (10, 1));
}

#[test]
fn a_flat_colour_survives_the_resample_unchanged() {
    let (w, h) = (64u32, 48);
    let rgb8: Vec<u8> = (0..w * h).flat_map(|_| [200u8, 90, 30]).collect();
    let out = resize_rgb(rgb8, (w, h), (16, 12), 8, Transfer::Srgb).expect("resize");
    assert_eq!(out.len(), 16 * 12 * 3);
    assert!(out.chunks(3).all(|p| p == [200, 90, 30]), "{:?}", &out[..6]);

    let rgb16: Vec<u8> = (0..w * h).flat_map(|_| [40000u16, 1200, 7].map(u16::to_le_bytes)).flatten().collect();
    let out = resize_rgb(rgb16, (w, h), (16, 12), 16, Transfer::Adobe).expect("resize");
    assert_eq!(out.len(), 16 * 12 * 6);
    for p in out.chunks(6) {
        let v: Vec<u16> = p.chunks(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        assert!(v[0].abs_diff(40000) <= 1 && v[1].abs_diff(1200) <= 1 && v[2].abs_diff(7) <= 1, "{v:?}");
    }
}

#[test]
fn fine_detail_is_averaged_in_linear_light() {
    // Alternating black and white columns average to half the light, which
    // sRGB writes as 188, not as the 128 a gamma-encoded average would give.
    let (w, h) = (64u32, 64);
    let rgb: Vec<u8> = (0..h)
        .flat_map(|_| (0..w).flat_map(|x| [if x % 2 == 0 { 0u8 } else { 255 }; 3]))
        .collect();
    let out = resize_rgb(rgb, (w, h), (8, 8), 8, Transfer::Srgb).expect("resize");
    let centre = out[(4 * 8 + 4) * 3];
    assert!(centre.abs_diff(188) <= 2, "{centre}");
}

#[test]
fn a_mismatched_buffer_is_refused() {
    assert!(resize_rgb(vec![0; 10], (4, 4), (2, 2), 8, Transfer::Srgb).is_err());
}

// ── File name ─────────────────────────────────────────────────────────────────

#[test]
fn the_file_name_takes_the_formats_extension_once() {
    assert_eq!(export_file_name("IMG_0042", ExportFormat::Jxl).unwrap(), "IMG_0042.jxl");
    assert_eq!(export_file_name("  roll 3  ", ExportFormat::Png).unwrap(), "roll 3.png");
    assert_eq!(export_file_name("frame.JPEG", ExportFormat::Jpeg).unwrap(), "frame.jpg");
    assert_eq!(export_file_name("frame.tif", ExportFormat::Tiff).unwrap(), "frame.tiff");
    assert_eq!(export_file_name("v1.2", ExportFormat::Webp).unwrap(), "v1.2.webp");
}

#[test]
fn a_name_no_platform_can_hold_is_refused() {
    for bad in ["", "   ", ".jxl", "..", "a/b", "a\\b", "a:b", "what?", "dot.", "tab\there"] {
        assert!(export_file_name(bad, ExportFormat::Jxl).is_err(), "{bad:?}");
    }
}
