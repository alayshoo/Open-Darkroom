// src-native/src/model/history.rs
//
// Undo and redo, as src/lib/history/history.svelte.ts keeps them.

use super::crop::Crop;
use super::curves::Curves;
use super::sliders::{Key, Value};

#[derive(Clone, Debug)]
pub enum Action {
    Slider { key: Key, old: Value, new: Value },
    /// Many sliders moving as one step, so a paste or a reset takes a single
    /// undo. The curves ride along when the set includes them.
    Sliders {
        old: Vec<(Key, Value)>,
        new: Vec<(Key, Value)>,
        curves: Option<(Curves, Curves)>,
    },
    /// One edit in the curves editor, from pointer down to pointer up.
    Curves { old: Curves, new: Curves },
    Crop { old: Crop, new: Crop },
}

#[derive(Default)]
pub struct History {
    stack: Vec<Action>,
    /// Points one past the last applied action.
    applied: usize,
}

impl History {
    pub fn push(&mut self, action: Action) {
        // Discard any "future" actions after the current one.
        self.stack.truncate(self.applied);
        self.stack.push(action);
        self.applied += 1;
    }

    pub fn undo(&mut self) -> Option<Action> {
        if self.applied == 0 {
            return None;
        }
        self.applied -= 1;
        Some(self.stack[self.applied].clone())
    }

    pub fn redo(&mut self) -> Option<Action> {
        if self.applied >= self.stack.len() {
            return None;
        }
        self.applied += 1;
        Some(self.stack[self.applied - 1].clone())
    }

    /// Drop every action matching `pred`, keeping the pointer on the same step.
    pub fn discard(&mut self, pred: impl Fn(&Action) -> bool) {
        let removed_before = self.stack[..self.applied].iter().filter(|a| pred(a)).count();
        self.stack.retain(|a| !pred(a));
        self.applied -= removed_before;
    }
}
