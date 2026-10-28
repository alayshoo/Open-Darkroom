// src-native/src/model/clipboard.rs
//
// Adjustments as a flat YAML mapping, as src/lib/utils/adjustmentsClipboard.ts
// writes and reads them, so the two apps can paste each other's copies.

use std::collections::HashMap;

use super::curves::{Curves, Point, CHANNELS, NAMES as CURVE_NAMES};
use super::sliders::{Key, Sliders, Value};

/// Drops the float noise animated values pick up, well below any slider's own step.
fn format_number(v: f64) -> String {
    let rounded = format!("{v:.6}").parse::<f64>().unwrap_or(v);
    let mut s = format!("{rounded}");
    if s == "-0" {
        s = "0".into();
    }
    s
}

fn format_value(v: Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Num(x) => format_number(x),
    }
}

fn curve_key(channel: usize) -> String {
    let name = CURVE_NAMES[channel];
    format!("curve{}{}", name[..1].to_uppercase(), &name[1..])
}

pub fn write(sliders: &Sliders, keys: &[Key], curves: Option<&Curves>) -> String {
    let mut lines: Vec<String> = keys
        .iter()
        .map(|&k| format!("{}: {}", k.name(), format_value(sliders.get(k))))
        .collect();
    if let Some(curves) = curves {
        for c in 0..CHANNELS {
            let points: Vec<String> = curves.0[c]
                .iter()
                .map(|p| format!("{},{}", format_number(p.x), format_number(p.y)))
                .collect();
            lines.push(format!("{}: {}", curve_key(c), points.join(" ")));
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// Lenient: takes comments, document markers, quotes and stray whitespace,
/// since the point of the format is that it can be edited by hand.
fn read_document(text: &str) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line == "---" || line == "..." {
            continue;
        }
        let Some(colon) = line.find(':') else { continue };
        let key = line[..colon].trim();
        let mut value = line[colon + 1..].trim();
        if let Some(comment) = value.find(" #") {
            value = value[..comment].trim();
        }
        let bytes = value.as_bytes();
        if value.len() >= 2 && (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[value.len() - 1] == bytes[0] {
            value = &value[1..value.len() - 1];
        }
        if !key.is_empty() {
            entries.insert(key.to_string(), value.to_string());
        }
    }
    entries
}

/// A curve that is malformed is skipped whole rather than repaired into
/// something the user did not write.
fn parse_curve(raw: &str) -> Option<Vec<Point>> {
    let mut points: Vec<Point> = Vec::new();
    for pair in raw.split_whitespace() {
        let mut it = pair.split(',');
        let x: f64 = it.next()?.parse().ok()?;
        let y: f64 = it.next()?.parse().ok()?;
        if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&y) {
            return None;
        }
        if points.last().is_some_and(|p| x <= p.x) {
            return None;
        }
        points.push(Point { x, y });
    }
    let ok = points.len() >= 2 && points[0].x == 0.0 && points[points.len() - 1].x == 1.0;
    ok.then_some(points)
}

pub struct Pasted {
    pub sliders: Vec<(Key, Value)>,
    pub curves: Vec<(usize, Vec<Point>)>,
}

/// Anything outside `keys` is dropped rather than rejected, so a document
/// written by an older or newer build still applies whatever the two share.
pub fn read(text: &str, keys: &[Key], with_curves: bool) -> Result<Pasted, String> {
    let entries = read_document(text);
    if entries.is_empty() {
        return Err("Clipboard does not contain a YAML mapping".into());
    }
    let mut sliders = Vec::new();
    for &key in keys {
        let Some(raw) = entries.get(key.name()) else { continue };
        if raw.is_empty() {
            continue;
        }
        match key {
            Key::Invert => match raw.to_lowercase().as_str() {
                "true" => sliders.push((key, Value::Bool(true))),
                "false" => sliders.push((key, Value::Bool(false))),
                _ => {}
            },
            Key::N(_) => {
                if let Ok(x) = raw.trim().parse::<f64>() {
                    if x.is_finite() {
                        sliders.push((key, Value::Num(x)));
                    }
                }
            }
        }
    }
    let mut curves = Vec::new();
    if with_curves {
        for c in 0..CHANNELS {
            if let Some(points) = entries.get(&curve_key(c)).and_then(|raw| parse_curve(raw)) {
                curves.push((c, points));
            }
        }
    }
    if sliders.is_empty() && curves.is_empty() {
        return Err("Clipboard YAML holds no adjustments for this mode".into());
    }
    Ok(Pasted { sliders, curves })
}
