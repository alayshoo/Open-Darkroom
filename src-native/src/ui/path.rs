// src-native/src/ui/path.rs
//
// The SVG icons the Tauri app draws inline, flattened into polylines egui can
// stroke. Only the path commands those icons use are understood.

use egui::{Pos2, Shape, Stroke, Vec2};

/// Each subpath as points, and whether it closes.
pub fn flatten(d: &str) -> Vec<(Vec<Pos2>, bool)> {
    let tokens = tokenize(d);
    let mut out: Vec<(Vec<Pos2>, bool)> = Vec::new();
    let mut current: Vec<Pos2> = Vec::new();
    let mut pos = Pos2::ZERO;
    let mut start = Pos2::ZERO;
    let mut last_ctrl: Option<Pos2> = None;
    let mut i = 0;
    let mut cmd = 'M';
    let num = |i: &mut usize| -> f32 {
        let v = match tokens.get(*i) {
            Some(Token::Num(v)) => *v,
            _ => 0.0,
        };
        *i += 1;
        v
    };
    while i < tokens.len() {
        if let Token::Cmd(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                if !current.is_empty() {
                    out.push((std::mem::take(&mut current), true));
                }
                pos = start;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { pos.to_vec2() } else { Vec2::ZERO };
        match cmd.to_ascii_uppercase() {
            'M' => {
                if current.len() > 1 {
                    out.push((std::mem::take(&mut current), false));
                }
                current.clear();
                pos = Pos2::new(num(&mut i), num(&mut i)) + base;
                start = pos;
                current.push(pos);
                cmd = if rel { 'l' } else { 'L' };
                last_ctrl = None;
            }
            'L' => {
                pos = Pos2::new(num(&mut i), num(&mut i)) + base;
                current.push(pos);
                last_ctrl = None;
            }
            'H' => {
                let x = num(&mut i) + base.x;
                pos = Pos2::new(x, pos.y);
                current.push(pos);
                last_ctrl = None;
            }
            'V' => {
                let y = num(&mut i) + base.y;
                pos = Pos2::new(pos.x, y);
                current.push(pos);
                last_ctrl = None;
            }
            'C' | 'S' => {
                let c1 = if cmd.to_ascii_uppercase() == 'C' {
                    Pos2::new(num(&mut i), num(&mut i)) + base
                } else {
                    last_ctrl.map_or(pos, |c| pos + (pos - c))
                };
                let c2 = Pos2::new(num(&mut i), num(&mut i)) + base;
                let end = Pos2::new(num(&mut i), num(&mut i)) + base;
                for s in 1..=12 {
                    let t = s as f32 / 12.0;
                    let u = 1.0 - t;
                    let p = pos.to_vec2() * (u * u * u) + c1.to_vec2() * (3.0 * u * u * t) + c2.to_vec2() * (3.0 * u * t * t) + end.to_vec2() * (t * t * t);
                    current.push(p.to_pos2());
                }
                last_ctrl = Some(c2);
                pos = end;
            }
            'A' => {
                let (rx, ry, rot) = (num(&mut i), num(&mut i), num(&mut i));
                let (large, sweep) = (num(&mut i) != 0.0, num(&mut i) != 0.0);
                let end = Pos2::new(num(&mut i), num(&mut i)) + base;
                arc(&mut current, pos, end, rx, ry, rot, large, sweep);
                pos = end;
                last_ctrl = None;
            }
            _ => i += 1,
        }
    }
    if current.len() > 1 {
        out.push((current, false));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<Pos2>, from: Pos2, to: Pos2, rx: f32, ry: f32, rot_deg: f32, large: bool, sweep: bool) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        out.push(to);
        return;
    }
    let phi = rot_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let d = (from - to) * 0.5;
    let x1 = cos * d.x + sin * d.y;
    let y1 = -sin * d.x + cos * d.y;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let mid = (from.to_vec2() + to.to_vec2()) * 0.5;
    let c = Vec2::new(cos * cx1 - sin * cy1, sin * cx1 + cos * cy1) + mid;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let t1 = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && dt > 0.0 {
        dt -= std::f32::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f32::consts::TAU;
    }
    let steps = ((dt.abs() / 0.15).ceil() as usize).max(2);
    for s in 1..=steps {
        let t = t1 + dt * s as f32 / steps as f32;
        let (st, ct) = t.sin_cos();
        let x = rx * ct;
        let y = ry * st;
        out.push(Pos2::new(cos * x - sin * y + c.x, sin * x + cos * y + c.y));
    }
}

enum Token {
    Cmd(char),
    Num(f32),
}

fn tokenize(d: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            out.push(Token::Cmd(c));
            i += 1;
        } else if c == '-' || c == '.' || c.is_ascii_digit() {
            let start = i;
            let mut seen_dot = c == '.';
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && i + 1 < chars.len() {
                    i += 1;
                    if chars[i] == '-' || chars[i] == '+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            let s: String = chars[start..i].iter().collect();
            out.push(Token::Num(s.parse().unwrap_or(0.0)));
        } else {
            i += 1;
        }
    }
    out
}

/// A path in a `view` box drawn into `rect`'s top-left at `scale`, mirrored
/// horizontally when `mirror` is set.
pub struct Placed {
    pub origin: Pos2,
    pub scale: f32,
    pub view_origin: Vec2,
    pub mirror_width: Option<f32>,
    /// Degrees clockwise about a point in the view box, as SVG's rotate().
    pub rotate: Option<(f32, Pos2)>,
}

impl Placed {
    pub fn new(origin: Pos2, scale: f32) -> Self {
        Self { origin, scale, view_origin: Vec2::ZERO, mirror_width: None, rotate: None }
    }

    fn map(&self, p: Pos2) -> Pos2 {
        let p = match self.rotate {
            Some((deg, c)) => {
                let (s, co) = deg.to_radians().sin_cos();
                let d = p - c;
                c + Vec2::new(d.x * co - d.y * s, d.x * s + d.y * co)
            }
            None => p,
        };
        let mut v = p.to_vec2() - self.view_origin;
        if let Some(w) = self.mirror_width {
            v.x = w - v.x;
        }
        self.origin + v * self.scale
    }

    pub fn stroke(&self, painter: &egui::Painter, d: &str, stroke: Stroke) {
        for (points, closed) in flatten(d) {
            let points: Vec<Pos2> = points.into_iter().map(|p| self.map(p)).collect();
            if closed {
                painter.add(Shape::closed_line(points, stroke));
            } else {
                painter.add(Shape::line(points, stroke));
            }
        }
    }

    /// Fills each subpath as a convex polygon, which is what the filled icons are.
    pub fn fill(&self, painter: &egui::Painter, d: &str, color: egui::Color32) {
        for (points, _) in flatten(d) {
            let points: Vec<Pos2> = points.into_iter().map(|p| self.map(p)).collect();
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
    }
}

/// A round-capped segment, which egui's lines do not have.
/// A rounded rectangle outline in the view box, as an SVG <rect rx>.
pub fn rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> String {
    format!(
        "M{} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}H{}A{r} {r} 0 0 1 {} {}V{}A{r} {r} 0 0 1 {} {}Z",
        x + r, y, x + w - r, x + w, y + r, y + h - r, x + w - r, y + h, x + r, x, y + h - r, y + r, x + r, y
    )
}

pub fn capped_line(painter: &egui::Painter, a: Pos2, b: Pos2, width: f32, color: egui::Color32) {
    painter.line_segment([a, b], Stroke::new(width, color));
    painter.circle_filled(a, width / 2.0, color);
    painter.circle_filled(b, width / 2.0, color);
}
