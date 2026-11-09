// src-tauri/src/metadata.rs
//
// What a file says about how it was shot. RAW files carry it in their own
// containers, which rawler reads; JPEG and PNG carry an EXIF block, which is a
// TIFF structure and goes through rawler's TIFF parser.

use std::io::Cursor;
use std::path::Path;

use image::ImageDecoder;
use rawler::decoders::RawDecodeParams;
use rawler::exif::Exif;
use rawler::formats::tiff::{GenericTiffReader, Rational, Value, IFD};
use rawler::formats::tiff::reader::TiffReader;
use rawler::rawsource::RawSource;
use rawler::tags::TiffCommonTag;
use serde::Serialize;

/// Each field is None when the file does not say.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMetadata {
    pub file_name: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    /// In millimetres.
    pub focal_length: Option<f64>,
    /// The f-number.
    pub aperture: Option<f64>,
    /// In seconds, as the fraction the camera recorded.
    pub exposure_time: Option<[u32; 2]>,
    pub iso: Option<u32>,
    /// In stops.
    pub exposure_bias: Option<f64>,
    /// As EXIF writes it: `YYYY:MM:DD HH:MM:SS`.
    pub date_taken: Option<String>,
}

/// Never fails: a file that cannot be read for metadata still opens, with
/// only its name known.
pub fn read(path: &Path) -> ImageMetadata {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let found = if crate::raw::is_raw_extension(path) {
        read_raw(path)
    } else {
        read_encoded(path)
    };
    ImageMetadata { file_name, ..found.unwrap_or_default() }
}

fn read_raw(path: &Path) -> Option<ImageMetadata> {
    let source = RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&source).ok()?;
    let raw = decoder.raw_metadata(&source, &RawDecodeParams::default()).ok()?;

    let mut out = from_exif(&raw.exif);
    out.make = non_empty(&raw.make).or(out.make);
    out.model = non_empty(&raw.model).or(out.model);
    // The lens database names lenses the EXIF block leaves blank.
    if out.lens.is_none() {
        out.lens = raw.lens.and_then(|l| non_empty(&format!("{} {}", l.lens_make, l.lens_model)));
    }
    Some(out)
}

fn read_encoded(path: &Path) -> Option<ImageMetadata> {
    let mut decoder = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    from_exif_block(&decoder.exif_metadata().ok()??)
}

/// An EXIF block as JPEG and PNG embed it: a TIFF header and its IFDs.
pub fn from_exif_block(block: &[u8]) -> Option<ImageMetadata> {
    let tiff = GenericTiffReader::new(&mut Cursor::new(block), 0, 0, None, &[]).ok()?;
    let root = tiff.root_ifd();
    let mut out = from_exif(&Exif::new(root).ok()?);
    out.make = ascii(root, TiffCommonTag::Make);
    out.model = ascii(root, TiffCommonTag::Model);
    Some(out)
}

fn from_exif(exif: &Exif) -> ImageMetadata {
    let lens = exif.lens_model.as_deref().and_then(non_empty).map(|model| {
        // Some cameras repeat the make in the model, others leave it out.
        match exif.lens_make.as_deref().and_then(non_empty) {
            Some(make) if !model.to_lowercase().starts_with(&make.to_lowercase()) => format!("{make} {model}"),
            _ => model,
        }
    });
    ImageMetadata {
        lens,
        focal_length: exif.focal_length.and_then(ratio),
        aperture: exif.fnumber.and_then(ratio),
        exposure_time: exif.exposure_time.filter(|t| t.n > 0 && t.d > 0).map(|t| [t.n, t.d]),
        iso: exif
            .iso_speed_ratings
            .map(u32::from)
            .or(exif.recommended_exposure_index)
            .or(exif.iso_speed)
            .filter(|&iso| iso > 0),
        exposure_bias: exif.exposure_bias.filter(|b| b.d != 0).map(|b| b.n as f64 / b.d as f64),
        date_taken: exif
            .date_time_original
            .as_deref()
            .or(exif.create_date.as_deref())
            .and_then(non_empty),
        ..Default::default()
    }
}

fn ratio(r: Rational) -> Option<f64> {
    (r.d != 0 && r.n != 0).then(|| r.n as f64 / r.d as f64)
}

fn ascii(ifd: &IFD, tag: TiffCommonTag) -> Option<String> {
    match &ifd.get_entry(tag)?.value {
        Value::Ascii(text) => text.strings().first().and_then(|s| non_empty(s)),
        _ => None,
    }
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
