// src-tauri/src/exif_writing.rs
//
// The shooting information an export carries, as EXIF. It is rebuilt from what
// the source said rather than copied, so nothing in the original block (an
// orientation, a thumbnail, maker notes) can contradict the rendered pixels.

use crate::metadata::ImageMetadata;

pub mod tag {
    pub const MAKE: u16 = 0x010F;
    pub const MODEL: u16 = 0x0110;
    pub const ORIENTATION: u16 = 0x0112;
    pub const SOFTWARE: u16 = 0x0131;
    pub const EXIF_IFD: u16 = 0x8769;
    pub const EXPOSURE_TIME: u16 = 0x829A;
    pub const F_NUMBER: u16 = 0x829D;
    pub const ISO: u16 = 0x8827;
    pub const EXIF_VERSION: u16 = 0x9000;
    pub const DATE_TIME_ORIGINAL: u16 = 0x9003;
    pub const EXPOSURE_BIAS: u16 = 0x9204;
    pub const FOCAL_LENGTH: u16 = 0x920A;
    pub const LENS_MODEL: u16 = 0xA434;
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExifValue {
    Ascii(String),
    Short(u16),
    Long(u32),
    Rational(u32, u32),
    SRational(i32, i32),
    Undefined(Vec<u8>),
}

impl ExifValue {
    fn field_type(&self) -> u16 {
        match self {
            Self::Ascii(_) => 2,
            Self::Short(_) => 3,
            Self::Long(_) => 4,
            Self::Rational(..) => 5,
            Self::Undefined(_) => 7,
            Self::SRational(..) => 10,
        }
    }

    fn count(&self) -> u32 {
        match self {
            Self::Ascii(s) => s.len() as u32 + 1,
            Self::Undefined(b) => b.len() as u32,
            _ => 1,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Ascii(s) => s.bytes().chain([0]).collect(),
            Self::Short(v) => v.to_le_bytes().to_vec(),
            Self::Long(v) => v.to_le_bytes().to_vec(),
            Self::Rational(n, d) => [n.to_le_bytes(), d.to_le_bytes()].concat(),
            Self::SRational(n, d) => [n.to_le_bytes(), d.to_le_bytes()].concat(),
            Self::Undefined(b) => b.clone(),
        }
    }
}

pub type ExifEntry = (u16, ExifValue);

/// The primary directory's tags and the EXIF sub-directory's, each in tag order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExifTags {
    pub primary: Vec<ExifEntry>,
    pub exif: Vec<ExifEntry>,
}

pub fn exif_tags(meta: &ImageMetadata) -> ExifTags {
    let ascii = |s: &Option<String>| s.as_deref().map(clean).filter(|s| !s.is_empty()).map(ExifValue::Ascii);

    let mut primary = Vec::new();
    if let Some(v) = ascii(&meta.make) {
        primary.push((tag::MAKE, v));
    }
    if let Some(v) = ascii(&meta.model) {
        primary.push((tag::MODEL, v));
    }
    // The pixels are written the way they are meant to be seen.
    primary.push((tag::ORIENTATION, ExifValue::Short(1)));
    primary.push((
        tag::SOFTWARE,
        ExifValue::Ascii(format!("Open Darkroom {}", env!("CARGO_PKG_VERSION"))),
    ));

    let mut exif = Vec::new();
    if let Some([n, d]) = meta.exposure_time.filter(|[n, d]| *n > 0 && *d > 0) {
        exif.push((tag::EXPOSURE_TIME, ExifValue::Rational(n, d)));
    }
    if let Some((n, d)) = meta.aperture.and_then(unsigned_rational) {
        exif.push((tag::F_NUMBER, ExifValue::Rational(n, d)));
    }
    if let Some(iso) = meta.iso.filter(|&iso| iso > 0) {
        exif.push((tag::ISO, ExifValue::Short(iso.min(u32::from(u16::MAX)) as u16)));
    }
    if let Some(v) = ascii(&meta.date_taken) {
        exif.push((tag::DATE_TIME_ORIGINAL, v));
    }
    if let Some((n, d)) = meta.exposure_bias.and_then(signed_rational) {
        exif.push((tag::EXPOSURE_BIAS, ExifValue::SRational(n, d)));
    }
    if let Some((n, d)) = meta.focal_length.and_then(unsigned_rational) {
        exif.push((tag::FOCAL_LENGTH, ExifValue::Rational(n, d)));
    }
    if let Some(v) = ascii(&meta.lens) {
        exif.push((tag::LENS_MODEL, v));
    }
    if !exif.is_empty() {
        exif.push((tag::EXIF_VERSION, ExifValue::Undefined(b"0232".to_vec())));
    }

    primary.sort_by_key(|(t, _)| *t);
    exif.sort_by_key(|(t, _)| *t);
    ExifTags { primary, exif }
}

/// A little-endian TIFF structure: the primary directory, then the EXIF one it
/// points at. This is what JPEG, PNG, WebP and JPEG XL each embed.
pub fn exif_block(tags: &ExifTags) -> Vec<u8> {
    let mut primary = tags.primary.clone();
    if !tags.exif.is_empty() {
        primary.push((tag::EXIF_IFD, ExifValue::Long(0)));
        primary.sort_by_key(|(t, _)| *t);
        let exif_at = 8 + directory_len(&primary);
        for entry in primary.iter_mut().filter(|(t, _)| *t == tag::EXIF_IFD) {
            entry.1 = ExifValue::Long(exif_at as u32);
        }
    }

    let mut out = b"II".to_vec();
    out.extend(42u16.to_le_bytes());
    out.extend(8u32.to_le_bytes());
    write_directory(&mut out, &primary);
    if !tags.exif.is_empty() {
        write_directory(&mut out, &tags.exif);
    }
    out
}

fn padded(len: usize) -> usize {
    len + len % 2
}

fn directory_len(entries: &[ExifEntry]) -> usize {
    let data: usize = entries
        .iter()
        .map(|(_, v)| v.bytes().len())
        .filter(|&n| n > 4)
        .map(padded)
        .sum();
    2 + 12 * entries.len() + 4 + data
}

/// Values longer than four bytes follow the directory, at offsets counted from
/// the start of the block.
fn write_directory(out: &mut Vec<u8>, entries: &[ExifEntry]) {
    let data_at = out.len() + 2 + 12 * entries.len() + 4;
    let mut data = Vec::new();

    out.extend((entries.len() as u16).to_le_bytes());
    for (tag, value) in entries {
        let bytes = value.bytes();
        out.extend(tag.to_le_bytes());
        out.extend(value.field_type().to_le_bytes());
        out.extend(value.count().to_le_bytes());
        if bytes.len() <= 4 {
            let mut field = [0u8; 4];
            field[..bytes.len()].copy_from_slice(&bytes);
            out.extend(field);
        } else {
            out.extend(((data_at + data.len()) as u32).to_le_bytes());
            data.extend(&bytes);
            if bytes.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(data);
}

/// EXIF's text fields are ASCII, so accented letters lose their accents and
/// anything else without a plain stand-in is left out. NUL ends a field early.
pub fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' '..='~' => out.push(c),
            '\u{A0}' => out.push(' '),
            'Æ' => out.push_str("AE"),
            'æ' => out.push_str("ae"),
            'Œ' => out.push_str("OE"),
            'œ' => out.push_str("oe"),
            'ß' => out.push_str("ss"),
            'Þ' => out.push_str("Th"),
            'þ' => out.push_str("th"),
            'Ĳ' => out.push_str("IJ"),
            'ĳ' => out.push_str("ij"),
            'µ' | 'μ' => out.push('u'),
            '×' => out.push('x'),
            '\u{C0}'..='\u{17F}' => {
                let folded = LATIN[c as usize - 0xC0];
                if folded != b'?' {
                    out.push(char::from(folded));
                }
            }
            _ => {}
        }
    }
    out.trim().to_string()
}

/// The unaccented letter for each of U+00C0 to U+017F; '?' marks the few handled
/// above or left out.
const LATIN: &[u8; 192] = b"AAAAAA?CEEEEIIIIDNOOOOO?OUUUUY??aaaaaa?ceeeeiiiidnooooo?ouuuuy?yAaAaAaCcCcCcCcDdDdEeEeEeEeEeGgGgGgGgHhHhIiIiIiIiIi??JjKk?LlLlLlLlLlNnNnNnn??OoOoOo??RrRrRrSsSsSsSsTtTtTtUuUuUuUuUuUuWwYyYZzZzZzs";

/// The smallest of a few usual denominators that holds `v` exactly, so f/2.8
/// stays 28/10 and a third of a stop stays 1/3.
fn fraction(v: f64) -> Option<(i64, i64)> {
    if !v.is_finite() {
        return None;
    }
    const DENOMINATORS: [i64; 8] = [1, 2, 3, 4, 6, 10, 100, 1000];
    let found = DENOMINATORS
        .iter()
        .map(|&d| ((v * d as f64).round() as i64, d))
        .find(|&(n, d)| (n as f64 / d as f64 - v).abs() < 1e-6)
        .unwrap_or(((v * 1000.0).round() as i64, 1000));
    Some(found)
}

fn unsigned_rational(v: f64) -> Option<(u32, u32)> {
    let (n, d) = fraction(v).filter(|&(n, _)| n > 0)?;
    Some((u32::try_from(n).ok()?, d as u32))
}

fn signed_rational(v: f64) -> Option<(i32, i32)> {
    let (n, d) = fraction(v)?;
    Some((i32::try_from(n).ok()?, d as i32))
}
