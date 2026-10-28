// src-native/src/ui/anim.rs
//
// CSS transitions for an immediate-mode UI: every animated value is keyed,
// and a change of target starts a tween from wherever the value is now, the
// way a CSS transition retargets mid-flight.

use std::collections::HashMap;

/// A cubic-bezier timing function, as CSS defines them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ease(pub f32, pub f32, pub f32, pub f32);

pub const EASE: Ease = Ease(0.25, 0.1, 0.25, 1.0);
pub const LINEAR: Ease = Ease(0.0, 0.0, 1.0, 1.0);
pub const EASE_OUT_EMPH: Ease = Ease(0.2, 0.0, 0.0, 1.0);
pub const SPRING: Ease = Ease(0.34, 1.56, 0.64, 1.0);
pub const EASE_IN_OUT: Ease = Ease(0.42, 0.0, 0.58, 1.0);

impl Ease {
    pub fn apply(self, x: f32) -> f32 {
        let Ease(x1, y1, x2, y2) = self;
        if self == LINEAR {
            return x;
        }
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let bez = |t: f32, a: f32, b: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        // Solve x(t) = x by bisection; plenty for a UI.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let mut t = x;
        for _ in 0..24 {
            let v = bez(t, x1, x2);
            if (v - x).abs() < 1e-5 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = (lo + hi) * 0.5;
        }
        bez(t, y1, y2)
    }
}

/// svelte's cubicInOut, which the pager slide uses.
pub fn cubic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let f = 2.0 * t - 2.0;
        0.5 * f * f * f + 1.0
    }
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f64,
    duration: f32,
    ease: Ease,
}

impl Tween {
    fn at(&self, now: f64) -> (f32, bool) {
        if self.duration <= 0.0 {
            return (self.to, false);
        }
        let t = ((now - self.start) as f32 / self.duration).clamp(0.0, 1.0);
        (self.from + (self.to - self.from) * self.ease.apply(t), t < 1.0)
    }
}

#[derive(Default)]
pub struct Anims {
    tweens: HashMap<egui::Id, Tween>,
    pub now: f64,
    pub running: bool,
}

impl Anims {
    pub fn begin_frame(&mut self, now: f64) {
        self.now = now;
        self.running = false;
    }

    /// The value keyed `id`, heading for `target` over `seconds`. The first
    /// time a key is seen it starts at its target, as a CSS property does.
    pub fn value(&mut self, id: impl Into<egui::Id>, target: f32, seconds: f32, ease: Ease) -> f32 {
        let id = id.into();
        let now = self.now;
        let tween = self.tweens.entry(id).or_insert(Tween { from: target, to: target, start: now, duration: 0.0, ease });
        if tween.to != target {
            let (current, _) = tween.at(now);
            *tween = Tween { from: current, to: target, start: now, duration: seconds, ease };
        }
        let (v, running) = tween.at(now);
        self.running |= running;
        v
    }

    /// 0 or 1, for a boolean that fades.
    pub fn flag(&mut self, id: impl Into<egui::Id>, on: bool, seconds: f32, ease: Ease) -> f32 {
        self.value(id, if on { 1.0 } else { 0.0 }, seconds, ease)
    }

    /// Restart a keyed value from `from`, for an entrance animation.
    pub fn restart(&mut self, id: impl Into<egui::Id>, from: f32, to: f32, seconds: f32, ease: Ease) {
        let now = self.now;
        self.tweens.insert(id.into(), Tween { from, to, start: now, duration: seconds, ease });
        self.running = true;
    }

    /// Jump straight to a value, with no transition.
    pub fn set(&mut self, id: impl Into<egui::Id>, value: f32) {
        let now = self.now;
        self.tweens.insert(id.into(), Tween { from: value, to: value, start: now, duration: 0.0, ease: LINEAR });
    }
}
