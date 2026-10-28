// src-native/src/model/curves.rs
//
// The point curves, as src/lib/types/curves.ts describes them.

/// Master, then red, green and blue.
pub const CHANNELS: usize = 4;
pub const MASTER: usize = 0;

/// Each curve's colour, in sRGB, shared by the editor and its histogram backdrop.
pub const COLOURS: [[u8; 3]; CHANNELS] = [
    [0xe6, 0xe6, 0xe6],
    [0xf0, 0x44, 0x3a],
    [0x4c, 0xbf, 0x4f],
    [0x3f, 0x6f, 0xf0],
];

pub const LABELS: [&str; CHANNELS] = ["RGB", "Red", "Green", "Blue"];

/// What the clipboard calls each curve.
pub const NAMES: [&str; CHANNELS] = ["master", "red", "green", "blue"];

/// Entries per channel in the baked table. Must match `curve_lut` in chunks/params.wgsl.
pub const LUT_SIZE: usize = 256;

/// Below this a point counts as sitting on the identity diagonal.
const IDENTITY_TOLERANCE: f64 = 1e-6;

/// A control point, both axes in encoded tone from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Each curve is sorted by x, and always starts at x = 0 and ends at x = 1.
pub type Curve = Vec<Point>;

#[derive(Clone, Debug, PartialEq)]
pub struct Curves(pub [Curve; CHANNELS]);

pub fn identity_curve() -> Curve {
    vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }]
}

impl Default for Curves {
    fn default() -> Self {
        Curves(std::array::from_fn(|_| identity_curve()))
    }
}

pub fn is_identity(points: &[Point]) -> bool {
    points.iter().all(|p| (p.x - p.y).abs() <= IDENTITY_TOLERANCE)
}

/// Monotone cubic interpolation (Fritsch–Carlson, with PCHIP's weighted
/// harmonic tangents). Passes through every point and never overshoots
/// between two of them.
pub struct Evaluator {
    xs: Vec<f64>,
    ys: Vec<f64>,
    h: Vec<f64>,
    m: Vec<f64>,
}

impl Evaluator {
    pub fn new(points: &[Point]) -> Self {
        let n = points.len();
        let xs: Vec<f64> = points.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = points.iter().map(|p| p.y).collect();
        let mut h = Vec::with_capacity(n);
        let mut d = Vec::with_capacity(n);
        for i in 0..n - 1 {
            h.push(xs[i + 1] - xs[i]);
            d.push((ys[i + 1] - ys[i]) / h[i]);
        }
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            // A turning point or a flat run gets a flat tangent, which is what
            // keeps the curve from overshooting its neighbours.
            if d[i - 1] * d[i] <= 0.0 {
                m[i] = 0.0;
            } else {
                let w1 = 2.0 * h[i] + h[i - 1];
                let w2 = h[i] + 2.0 * h[i - 1];
                m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
            }
        }
        Self { xs, ys, h, m }
    }

    pub fn eval(&self, x: f64) -> f64 {
        let n = self.xs.len();
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let mut i = 0;
        while x > self.xs[i + 1] {
            i += 1;
        }
        let t = (x - self.xs[i]) / self.h[i];
        let t2 = t * t;
        let t3 = t2 * t;
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.ys[i]
            + (t3 - 2.0 * t2 + t) * self.h[i] * self.m[i]
            + (-2.0 * t3 + 3.0 * t2) * self.ys[i + 1]
            + (t3 - t2) * self.h[i] * self.m[i + 1]
    }
}

/// The table the develop chain samples: per input level, the colour curve then
/// the master curve, as r, g, b and an unused fourth lane. None when every
/// curve that applies is the identity, which lets the chain skip the step.
///
/// `with_colour` false leaves the colour curves out, as B&W does.
pub fn bake_lut(curves: &Curves, with_colour: bool) -> Option<Vec<f32>> {
    let colour_active = with_colour && (1..CHANNELS).any(|c| !is_identity(&curves.0[c]));
    if !colour_active && is_identity(&curves.0[MASTER]) {
        return None;
    }
    let master = Evaluator::new(&curves.0[MASTER]);
    let channels: Vec<Option<Evaluator>> = (1..CHANNELS)
        .map(|c| colour_active.then(|| Evaluator::new(&curves.0[c])))
        .collect();

    let mut lut = vec![0.0f32; LUT_SIZE * 4];
    for i in 0..LUT_SIZE {
        let x = i as f64 / (LUT_SIZE - 1) as f64;
        for c in 0..3 {
            let v = channels[c].as_ref().map_or(x, |e| e.eval(x));
            lut[i * 4 + c] = master.eval(v) as f32;
        }
    }
    Some(lut)
}
