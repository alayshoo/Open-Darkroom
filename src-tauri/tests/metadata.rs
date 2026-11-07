// src-tauri/tests/metadata.rs
//
// Layer 1: what a file says about how it was shot. An EXIF block is written
// into real JPEG and PNG files through the image crate's own encoders, then
// read back from disk the way an open reads it.

use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

use open_darkroom_lib::metadata::{from_exif_block, read, ImageMetadata};

// ── A little-endian EXIF block ────────────────────────────────────────────────

enum Field {
    Ascii(&'static str),
    Short(u16),
    Long(u32),
    Rational(u32, u32),
    SRational(i32, i32),
}

impl Field {
    fn kind_count_bytes(&self) -> (u16, u32, Vec<u8>) {
        match self {
            Field::Ascii(s) => {
                let mut b = s.as_bytes().to_vec();
                b.push(0);
                (2, b.len() as u32, b)
            }
            Field::Short(v) => (3, 1, v.to_le_bytes().to_vec()),
            Field::Long(v) => (4, 1, v.to_le_bytes().to_vec()),
            Field::Rational(n, d) => (5, 1, [n.to_le_bytes(), d.to_le_bytes()].concat()),
            Field::SRational(n, d) => (10, 1, [n.to_le_bytes(), d.to_le_bytes()].concat()),
        }
    }
}

/// One IFD laid out at `at`, its long values right after it.
fn ifd(at: u32, fields: &[(u16, Field)]) -> Vec<u8> {
    let table = 2 + fields.len() as u32 * 12 + 4;
    let mut head = (fields.len() as u16).to_le_bytes().to_vec();
    let mut data = Vec::new();
    for (tag, field) in fields {
        let (kind, count, bytes) = field.kind_count_bytes();
        head.extend(tag.to_le_bytes());
        head.extend(kind.to_le_bytes());
        head.extend(count.to_le_bytes());
        if bytes.len() <= 4 {
            let mut inline = bytes.clone();
            inline.resize(4, 0);
            head.extend(inline);
        } else {
            head.extend((at + table + data.len() as u32).to_le_bytes());
            data.extend(&bytes);
            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    head.extend(0u32.to_le_bytes());
    head.extend(data);
    head
}

/// A TIFF header, IFD0 with the camera, and the EXIF IFD with the exposure.
fn exif_block(exposure: &[(u16, Field)]) -> Vec<u8> {
    let camera = |exif_at: u32| {
        vec![
            (0x010F, Field::Ascii("FUJIFILM")),
            (0x0110, Field::Ascii("X-T3")),
            (0x8769, Field::Long(exif_at)),
        ]
    };
    // Laid out once to learn its length, then again pointing past itself.
    let len = ifd(8, &camera(0)).len() as u32;
    let mut block = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
    block.extend(ifd(8, &camera(8 + len)));
    block.extend(ifd(8 + len, exposure));
    block
}

fn full_exposure() -> Vec<(u16, Field)> {
    vec![
        (0x829A, Field::Rational(1, 250)),
        (0x829D, Field::Rational(28, 10)),
        (0x8827, Field::Short(160)),
        (0x9003, Field::Ascii("2024:03:12 14:03:22")),
        (0x9204, Field::SRational(-2, 3)),
        (0x920A, Field::Rational(23, 1)),
        (0xA433, Field::Ascii("FUJIFILM")),
        (0xA434, Field::Ascii("XF23mmF2 R WR")),
    ]
}

// ── Files on disk ─────────────────────────────────────────────────────────────

const SIZE: u32 = 4;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("open-darkroom-metadata-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn write_jpeg(path: &Path, exif: Option<Vec<u8>>) {
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new(&mut bytes);
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).unwrap();
    }
    encoder
        .write_image(&[128u8; (SIZE * SIZE * 3) as usize], SIZE, SIZE, ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn write_png(path: &Path, exif: Vec<u8>) {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .write_image(&[128u8; (SIZE * SIZE * 3) as usize], SIZE, SIZE, ExtendedColorType::Rgb8)
        .unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn shot() -> ImageMetadata {
    ImageMetadata {
        file_name: String::new(),
        make: Some("FUJIFILM".into()),
        model: Some("X-T3".into()),
        lens: Some("FUJIFILM XF23mmF2 R WR".into()),
        focal_length: Some(23.0),
        aperture: Some(2.8),
        exposure_time: Some([1, 250]),
        iso: Some(160),
        exposure_bias: Some(-2.0 / 3.0),
        date_taken: Some("2024:03:12 14:03:22".into()),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn reads_a_jpeg_exif_block() {
    let path = scratch("shot.jpg");
    write_jpeg(&path, Some(exif_block(&full_exposure())));
    assert_eq!(read(&path), ImageMetadata { file_name: "shot.jpg".into(), ..shot() });
}

#[test]
fn reads_a_png_exif_chunk() {
    let path = scratch("shot.png");
    write_png(&path, exif_block(&full_exposure()));
    assert_eq!(read(&path), ImageMetadata { file_name: "shot.png".into(), ..shot() });
}

#[test]
fn a_file_without_exif_gives_only_its_name() {
    let path = scratch("plain.jpg");
    write_jpeg(&path, None);
    assert_eq!(read(&path), ImageMetadata { file_name: "plain.jpg".into(), ..Default::default() });
}

#[test]
fn an_unreadable_file_gives_only_its_name() {
    let path = scratch("broken.jpg");
    std::fs::write(&path, b"not an image").unwrap();
    assert_eq!(read(&path), ImageMetadata { file_name: "broken.jpg".into(), ..Default::default() });
}

#[test]
fn garbage_is_no_exif_block() {
    assert_eq!(from_exif_block(b"nothing here"), None);
}

#[test]
fn names_the_lens_maker_only_when_the_model_leaves_it_out() {
    let mut exposure = full_exposure();
    exposure.retain(|(tag, _)| *tag != 0xA433 && *tag != 0xA434);
    exposure.push((0xA433, Field::Ascii("Sigma")));
    exposure.push((0xA434, Field::Ascii("35mm F1.4 DG HSM | A")));
    let got = from_exif_block(&exif_block(&exposure)).unwrap();
    assert_eq!(got.lens.as_deref(), Some("Sigma 35mm F1.4 DG HSM | A"));

    let mut exposure = full_exposure();
    exposure.retain(|(tag, _)| *tag != 0xA434);
    exposure.push((0xA434, Field::Ascii("Fujifilm XF23mmF2 R WR")));
    let got = from_exif_block(&exif_block(&exposure)).unwrap();
    assert_eq!(got.lens.as_deref(), Some("Fujifilm XF23mmF2 R WR"));
}

#[test]
fn leaves_out_values_a_camera_writes_as_zero() {
    let got = from_exif_block(&exif_block(&[
        (0x829A, Field::Rational(0, 1)),
        (0x829D, Field::Rational(0, 0)),
        (0x8827, Field::Short(0)),
        (0x920A, Field::Rational(0, 1)),
        (0xA434, Field::Ascii("  ")),
    ]))
    .unwrap();
    assert_eq!(got, ImageMetadata { make: Some("FUJIFILM".into()), model: Some("X-T3".into()), ..Default::default() });
}
