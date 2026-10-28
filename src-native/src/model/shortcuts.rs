// src-native/src/model/shortcuts.rs
//
// Every rebindable action, the keys it answers to, and the part of that the
// user has changed. Mirrors src/lib/config/shortcutRegistry.ts and
// src/lib/settings/shortcuts.svelte.ts; bindings are the same strings, so a
// rebind made in either app holds in the other.

use std::collections::HashMap;

use super::settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutId {
    PagePrev,
    PageNext,
    ChangeMode,
    OpenImage,
    ToggleSettings,
    Undo,
    Redo,
    CopyAdjustments,
    PasteAdjustments,
    ToggleCrop,
    ResetAdjustments,
    ToggleDebugStats,
    ToggleHistogram,
    ToggleColorSpace,
}

pub struct ShortcutAction {
    pub id: ShortcutId,
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub group: &'static str,
    pub defaults: &'static [&'static str],
}

pub struct ShortcutGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub note: &'static str,
}

pub const GROUPS: [ShortcutGroup; 4] = [
    ShortcutGroup {
        id: "navigation",
        label: "Navigation",
        note: "The mouse side buttons page the strip as well, and are fixed.",
    },
    ShortcutGroup { id: "file", label: "File", note: "" },
    ShortcutGroup { id: "edit", label: "Edit", note: "" },
    ShortcutGroup { id: "debug", label: "Debug", note: "Live only while the debug tools are on." },
];

macro_rules! action {
    ($id:ident, $key:literal, $group:literal, $label:literal, $desc:literal, [$($d:literal),*]) => {
        ShortcutAction {
            id: ShortcutId::$id,
            key: $key,
            group: $group,
            label: $label,
            description: $desc,
            defaults: &[$($d),*],
        }
    };
}

pub const ACTIONS: [ShortcutAction; 14] = [
    action!(PagePrev, "pagePrev", "navigation", "Previous tab", "", [",", "a"]),
    action!(PageNext, "pageNext", "navigation", "Next tab", "", [".", "d"]),
    action!(ChangeMode, "changeMode", "navigation", "Switch mode", "Between Dark Room mode and Light Table mode.", ["m", "s"]),
    action!(OpenImage, "openImage", "file", "Open image", "", ["ctrl+o"]),
    action!(ToggleSettings, "toggleSettings", "file", "Settings", "Opens this window, and closes it again.", ["ctrl+,"]),
    action!(Undo, "undo", "edit", "Undo", "", ["ctrl+z"]),
    action!(Redo, "redo", "edit", "Redo", "", ["ctrl+shift+z"]),
    action!(CopyAdjustments, "copyAdjustments", "edit", "Copy adjustments", "", ["ctrl+c"]),
    action!(PasteAdjustments, "pasteAdjustments", "edit", "Paste adjustments", "", ["ctrl+v"]),
    action!(ToggleCrop, "toggleCrop", "edit", "Crop", "Enters the crop tool, and applies the crop on the way out.", ["r"]),
    action!(ResetAdjustments, "resetAdjustments", "edit", "Reset adjustments", "", ["ctrl+alt+r"]),
    action!(ToggleDebugStats, "toggleDebugStats", "debug", "Frame stats overlay", "", ["F1"]),
    action!(ToggleHistogram, "toggleHistogram", "debug", "Live histogram", "", ["F2"]),
    action!(ToggleColorSpace, "toggleColorSpace", "debug", "Colour space panel", "The highlight roll-off, the chromaticity diagram and the colour volume, on the left.", ["F3"]),
];

pub fn action(id: ShortcutId) -> &'static ShortcutAction {
    ACTIONS.iter().find(|a| a.id == id).expect("every id is registered")
}

pub fn actions_in(group: &str) -> impl Iterator<Item = &'static ShortcutAction> + '_ {
    ACTIONS.iter().filter(move |a| a.group == group)
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Binding {
    /// Single characters lowercased; anything longer as KeyboardEvent.key spells it.
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

fn normalise_key(key: &str) -> String {
    if key.chars().count() == 1 {
        key.to_lowercase()
    } else {
        key.to_string()
    }
}

pub fn serialize(b: &Binding) -> String {
    let mut parts = Vec::new();
    if b.ctrl {
        parts.push("ctrl".to_string());
    }
    if b.alt {
        parts.push("alt".to_string());
    }
    if b.shift {
        parts.push("shift".to_string());
    }
    if b.meta {
        parts.push("meta".to_string());
    }
    parts.push(b.key.clone());
    parts.join("+")
}

/// Lenient enough to survive a hand-edited file. Modifier names are taken off
/// the front until one is not recognised; whatever is left is the key.
pub fn parse(serialized: &str) -> Option<Binding> {
    let parts: Vec<&str> = serialized.split('+').collect();
    let mut b = Binding::default();
    let mut i = 0;
    while i + 1 < parts.len() {
        match parts[i].to_lowercase().as_str() {
            "ctrl" => b.ctrl = true,
            "shift" => b.shift = true,
            "alt" => b.alt = true,
            "meta" => b.meta = true,
            _ => break,
        }
        i += 1;
    }
    let key = parts[i..].join("+");
    if key.is_empty() {
        return None;
    }
    b.key = normalise_key(&key);
    Some(b)
}

/// How a binding reads on a chip or in a menu: "Ctrl+Shift+Z".
pub fn display(serialized: &str) -> String {
    let Some(b) = parse(serialized) else { return String::new() };
    let mut parts: Vec<String> = Vec::new();
    if b.ctrl {
        parts.push("Ctrl".into());
    }
    if b.alt {
        parts.push("Alt".into());
    }
    if b.shift {
        parts.push("Shift".into());
    }
    if b.meta {
        parts.push("Meta".into());
    }
    let label = match b.key.as_str() {
        " " => "Space".to_string(),
        "ArrowUp" => "↑".into(),
        "ArrowDown" => "↓".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        "Escape" => "Esc".into(),
        "Backspace" => "⌫".into(),
        "Delete" => "Del".into(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_string(),
    };
    parts.push(label);
    parts.join("+")
}

/// A key press, as the binding strings spell it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Press {
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Press {
    pub fn binding(&self) -> Binding {
        Binding {
            key: normalise_key(&self.key),
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
            meta: self.meta,
        }
    }
}

/// Modifiers have to agree exactly, not merely be present.
pub fn matches(serialized: &str, p: &Press) -> bool {
    parse(serialized).is_some_and(|b| b == p.binding())
}

pub struct Shortcuts {
    custom: HashMap<ShortcutId, Vec<String>>,
    /// Set while a row in the settings tab is listening for a key press.
    pub recording: Option<ShortcutId>,
}

fn id_from_key(key: &str) -> Option<ShortcutId> {
    ACTIONS.iter().find(|a| a.key == key).map(|a| a.id)
}

fn same_set(a: &[String], b: &[&str]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(&x.as_str()))
}

impl Shortcuts {
    pub fn load(stored: &HashMap<String, Vec<String>>) -> Self {
        let mut custom = HashMap::new();
        for (key, list) in stored {
            let Some(id) = id_from_key(key) else { continue };
            let mut clean: Vec<String> = Vec::new();
            for b in list {
                if parse(b).is_some() && !clean.contains(b) {
                    clean.push(b.clone());
                }
            }
            custom.insert(id, clean);
        }
        Self { custom, recording: None }
    }

    /// Every binding for an action, custom if there is one and default if not.
    pub fn bindings(&self, id: ShortcutId) -> Vec<String> {
        match self.custom.get(&id) {
            Some(list) => list.clone(),
            None => action(id).defaults.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn is_custom(&self, id: ShortcutId) -> bool {
        self.custom.contains_key(&id)
    }

    pub fn is_any_custom(&self) -> bool {
        !self.custom.is_empty()
    }

    /// The first binding, formatted for a menu row. Empty if there is none.
    pub fn display(&self, id: ShortcutId) -> String {
        self.bindings(id).first().map(|b| display(b)).unwrap_or_default()
    }

    pub fn action_for(&self, p: &Press) -> Option<ShortcutId> {
        ACTIONS
            .iter()
            .find(|a| self.bindings(a.id).iter().any(|b| matches(b, p)))
            .map(|a| a.id)
    }

    /// Gives a binding to an action, taking it off whatever else held it.
    /// Returns the labels it was taken from.
    pub fn add(&mut self, id: ShortcutId, binding: &str) -> Vec<&'static str> {
        let mut stolen = Vec::new();
        for a in ACTIONS.iter() {
            if a.id == id {
                continue;
            }
            let held = self.bindings(a.id);
            if !held.iter().any(|b| b == binding) {
                continue;
            }
            self.write(a.id, held.into_iter().filter(|b| b != binding).collect());
            stolen.push(a.label);
        }
        let mut held = self.bindings(id);
        if !held.iter().any(|b| b == binding) {
            held.push(binding.to_string());
            self.write(id, held);
        }
        self.persist();
        stolen
    }

    pub fn remove(&mut self, id: ShortcutId, binding: &str) {
        let held = self.bindings(id).into_iter().filter(|b| b != binding).collect();
        self.write(id, held);
        self.persist();
    }

    pub fn reset(&mut self, id: ShortcutId) {
        self.custom.remove(&id);
        self.persist();
    }

    pub fn reset_all(&mut self) {
        self.custom.clear();
        self.persist();
    }

    /// An action put back the way it shipped stops being an override.
    fn write(&mut self, id: ShortcutId, bindings: Vec<String>) {
        if same_set(&bindings, action(id).defaults) {
            self.custom.remove(&id);
        } else {
            self.custom.insert(id, bindings);
        }
    }

    fn persist(&self) {
        let mut map = serde_json::Map::new();
        for a in ACTIONS.iter() {
            if let Some(list) = self.custom.get(&a.id) {
                map.insert(a.key.to_string(), serde_json::json!(list));
            }
        }
        settings::save("shortcuts", serde_json::Value::Object(map));
    }
}
