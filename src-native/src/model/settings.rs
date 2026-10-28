// src-native/src/model/settings.rs
//
// Preferences that outlive a session. The file is the one the Tauri app's
// store plugin keeps under the identifier's app-data directory, so the two
// builds read and write the same preferences.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Map, Value};

const IDENTIFIER: &str = "com.Layshoo.open-darkroom";
pub const SETTINGS_FILE: &str = "settings.json";

fn path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(IDENTIFIER).join(SETTINGS_FILE))
}

pub struct Persisted {
    pub debug_tools: bool,
    /// Rebound keyboard shortcuts, keyed by action id — only the actions the
    /// user has actually changed.
    pub shortcuts: HashMap<String, Vec<String>>,
}

impl Default for Persisted {
    fn default() -> Self {
        Self { debug_tools: cfg!(debug_assertions), shortcuts: HashMap::new() }
    }
}

fn read_map() -> Map<String, Value> {
    let Some(path) = path() else { return Map::new() };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// The stored value for every key whose recorded type still matches its
/// default. One bad line costs one preference rather than all of them.
pub fn load() -> Persisted {
    let map = read_map();
    let mut p = Persisted::default();
    if let Some(Value::Bool(b)) = map.get("debugTools") {
        p.debug_tools = *b;
    }
    if let Some(Value::Object(o)) = map.get("shortcuts") {
        for (id, list) in o {
            if let Value::Array(items) = list {
                let bindings = items.iter().filter_map(|b| b.as_str().map(str::to_string)).collect();
                p.shortcuts.insert(id.clone(), bindings);
            }
        }
    }
    p
}

/// Records one preference, flushed at once — a toggle is usually the last
/// thing that happens before the window closes. Failing to write is not worth
/// interrupting an edit over.
pub fn save(key: &str, value: Value) {
    let Some(path) = path() else { return };
    let mut map = read_map();
    map.insert(key.to_string(), value);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&Value::Object(map)) {
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("Settings could not reach {}: {e}", path.display());
        }
    }
}
