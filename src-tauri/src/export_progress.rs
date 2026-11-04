// src-tauri/src/export_progress.rs
//
// Turns the render's and the encoder's own progress into one bar. Encoding is
// given the share of the bar its estimated cost bears to the render's, which
// is measured from how long the first chunk took.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Preparing,
    Rendering,
    Encoding,
}

#[derive(serde::Serialize, Clone, Copy, Debug, PartialEq)]
pub struct ProgressEvent {
    pub phase: Phase,
    /// Fraction of the whole export done, 0 – 1. Never decreases.
    pub progress: f32,
    /// The current phase cannot say how far along it is.
    pub indeterminate: bool,
}

/// Steps smaller than this are not worth an event.
const MIN_STEP: f32 = 0.005;

/// Encoding never takes the whole bar, so the render always visibly moves.
const MAX_ENCODE_SHARE: f32 = 0.95;

pub struct ProgressPlan {
    encode_seconds: f32,
    state: Mutex<State>,
}

struct State {
    render_started: Option<Instant>,
    encode_share: Option<f32>,
    last: Option<ProgressEvent>,
}

impl ProgressPlan {
    /// `encode_seconds` is the estimated time to encode the whole image.
    pub fn new(encode_seconds: f32) -> Self {
        ProgressPlan {
            encode_seconds: encode_seconds.max(0.0),
            state: Mutex::new(State { render_started: None, encode_share: None, last: None }),
        }
    }

    pub fn preparing(&self) -> Option<ProgressEvent> {
        self.emit(Phase::Preparing, 0.0, true)
    }

    /// The render reports 0 once it is set up, then the fraction of rows done.
    pub fn rendered(&self, fraction: f32) -> Option<ProgressEvent> {
        let now = Instant::now();
        let elapsed = {
            let mut state = self.state.lock().unwrap();
            let started = *state.render_started.get_or_insert(now);
            now - started
        };
        self.rendered_after(fraction, elapsed)
    }

    /// As [`Self::rendered`], with the time since the render started given.
    pub fn rendered_after(&self, fraction: f32, elapsed: Duration) -> Option<ProgressEvent> {
        let fraction = fraction.clamp(0.0, 1.0);
        let share = {
            let mut state = self.state.lock().unwrap();
            if state.encode_share.is_none() && fraction > 0.0 {
                let render_seconds = elapsed.as_secs_f32() / fraction;
                state.encode_share = Some(encode_share(render_seconds, self.encode_seconds));
            }
            state.encode_share.unwrap_or(0.0)
        };
        self.emit(Phase::Rendering, fraction * (1.0 - share), false)
    }

    /// Encoding has begun; `measurable` says whether [`Self::encoded`] will follow.
    pub fn encoding(&self, measurable: bool) -> Option<ProgressEvent> {
        self.emit(Phase::Encoding, 1.0 - self.share(), !measurable)
    }

    pub fn encoded(&self, fraction: f32) -> Option<ProgressEvent> {
        let share = self.share();
        self.emit(Phase::Encoding, 1.0 - share + share * fraction.clamp(0.0, 1.0), false)
    }

    fn share(&self) -> f32 {
        // A render too quick to report a chunk leaves the share unmeasured; the
        // estimate alone then decides, against a render of no length.
        let mut state = self.state.lock().unwrap();
        *state.encode_share.get_or_insert(encode_share(0.0, self.encode_seconds))
    }

    fn emit(&self, phase: Phase, progress: f32, indeterminate: bool) -> Option<ProgressEvent> {
        let mut state = self.state.lock().unwrap();
        let previous = state.last;
        let progress = progress.max(previous.map_or(0.0, |p| p.progress)).min(1.0);
        let event = ProgressEvent { phase, progress, indeterminate };

        let worth_sending = match previous {
            None => true,
            Some(p) => {
                p.phase != phase
                    || p.indeterminate != indeterminate
                    || progress - p.progress >= MIN_STEP
                    || (progress >= 1.0 && p.progress < 1.0)
            }
        };
        if worth_sending {
            state.last = Some(event);
            Some(event)
        } else {
            None
        }
    }
}

/// The part of the bar encoding gets, from the two phases' expected durations.
pub fn encode_share(render_seconds: f32, encode_seconds: f32) -> f32 {
    let total = render_seconds.max(0.0) + encode_seconds.max(0.0);
    if total <= 0.0 {
        return 0.0;
    }
    (encode_seconds.max(0.0) / total).min(MAX_ENCODE_SHARE)
}
