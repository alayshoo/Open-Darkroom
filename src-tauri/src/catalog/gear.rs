// src-tauri/src/catalog/gear.rs
//
// The cameras, lenses, scanners, chemistry and labs a roll can be shot,
// scanned and developed with. Kept in the catalog so they travel with it.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{err, Catalog};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gear {
    pub id: i64,
    #[serde(flatten)]
    pub fields: GearFields,
}

/// Everything about a piece of gear but its id, as the frontend edits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GearFields {
    /// `camera`, `lens`, `scanner`, `chemistry` or `lab`.
    pub kind: String,
    pub maker: String,
    pub name: String,
    #[serde(flatten)]
    pub details: GearDetails,
}

/// What only some kinds of gear have: a mount for cameras and lenses, formats
/// for cameras, a process for chemistry and a location for labs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GearDetails {
    pub mount: String,
    pub formats: Vec<String>,
    pub process: String,
    pub location: String,
}

impl GearFields {
    /// How a roll names it: the maker, then the model.
    pub fn label(&self) -> String {
        if self.maker.is_empty() { self.name.clone() } else { format!("{} {}", self.maker, self.name) }
    }
}

/// The gear a roll was shot, developed and scanned with, by id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RollGear {
    pub camera: Option<i64>,
    pub lenses: Vec<i64>,
    pub scanner: Option<i64>,
    pub chemistry: Option<i64>,
    pub lab: Option<i64>,
}

impl RollGear {
    fn uses(&self, id: i64) -> bool {
        self.camera == Some(id) || self.scanner == Some(id) || self.chemistry == Some(id) || self.lab == Some(id) || self.lenses.contains(&id)
    }
}

impl Catalog {
    pub fn gear(&self) -> Result<Vec<Gear>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, maker, name, details FROM gear ORDER BY maker COLLATE NOCASE, name COLLATE NOCASE, id")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                let details: String = r.get(4)?;
                Ok(Gear {
                    id: r.get(0)?,
                    fields: GearFields {
                        kind: r.get(1)?,
                        maker: r.get(2)?,
                        name: r.get(3)?,
                        details: serde_json::from_str(&details).unwrap_or_default(),
                    },
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    pub fn create_gear(&self, gear: &GearFields) -> Result<Gear, String> {
        self.conn
            .execute(
                "INSERT INTO gear (kind, maker, name, details) VALUES (?1, ?2, ?3, ?4)",
                params![gear.kind, gear.maker, gear.name, serde_json::to_string(&gear.details).map_err(err)?],
            )
            .map_err(err)?;
        Ok(Gear { id: self.conn.last_insert_rowid(), fields: gear.clone() })
    }

    pub fn update_gear(&self, id: i64, gear: &GearFields) -> Result<(), String> {
        let changed = self
            .conn
            .execute(
                "UPDATE gear SET kind = ?1, maker = ?2, name = ?3, details = ?4 WHERE id = ?5",
                params![gear.kind, gear.maker, gear.name, serde_json::to_string(&gear.details).map_err(err)?, id],
            )
            .map_err(err)?;
        if changed == 0 {
            return Err(format!("No gear {id} in the catalog"));
        }
        Ok(())
    }

    /// Removes a piece of gear. The rolls that used it keep its name in their information.
    pub fn delete_gear(&self, id: i64) -> Result<(), String> {
        let gear = self
            .gear()?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| format!("No gear {id} in the catalog"))?
            .fields;
        let label = gear.label();
        self.atomically(|| {
            for roll in self.rolls()?.into_iter().filter(|r| r.gear.uses(id)) {
                let mut links = roll.gear;
                let mut info = roll.info;
                let Value::Object(fields) = &mut info else { continue };
                let text = |key: &str| fields.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
                if links.lenses.contains(&id) {
                    links.lenses.retain(|&l| l != id);
                    let others = text("lenses");
                    let joined = if others.is_empty() { label.clone() } else { format!("{others}, {label}") };
                    fields.insert("lenses".into(), Value::String(joined));
                }
                for (slot, key) in [(&mut links.camera, "camera"), (&mut links.scanner, "scanner"), (&mut links.chemistry, "developer"), (&mut links.lab, "lab")] {
                    if *slot == Some(id) {
                        *slot = None;
                        fields.insert(key.into(), Value::String(label.clone()));
                    }
                }
                self.conn
                    .execute(
                        "UPDATE rolls SET info = ?1, gear = ?2 WHERE id = ?3",
                        params![info.to_string(), serde_json::to_string(&links).map_err(err)?, roll.id],
                    )
                    .map_err(err)?;
            }
            self.conn.execute("DELETE FROM gear WHERE id = ?1", params![id]).map(|_| ()).map_err(err)
        })
    }
}
