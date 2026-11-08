// src-tauri/src/catalog/film_stocks.rs
//
// The film stocks a roll can be shot on: kept in the catalog so they travel
// with it, and seeded once with common Kodak, Fujifilm and CineStill stocks.
// The colours are for telling rolls apart at a glance.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{err, Catalog};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilmStock {
    pub id: i64,
    #[serde(flatten)]
    pub fields: StockFields,
}

/// Everything about a stock but its id, as the frontend edits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockFields {
    pub maker: String,
    pub name: String,
    pub box_iso: Option<u32>,
    /// `colourNegative`, `slide` or `blackAndWhite`.
    pub kind: String,
    /// `C-41`, `E-6`, `B&W` or `ECN-2`.
    pub process: String,
    /// Any of `35mm`, `120`, `Sheet` and `16mm`.
    pub formats: Vec<String>,
    /// `#rrggbb`: the box's main colour, and up to two more that mark it.
    pub colour: Option<String>,
    pub colour2: Option<String>,
    pub colour3: Option<String>,
}

impl StockFields {
    /// How a roll names the stock: the maker, then the stock.
    pub fn label(&self) -> String {
        if self.maker.is_empty() { self.name.clone() } else { format!("{} {}", self.maker, self.name) }
    }
}

/// Maker, name, box ISO, kind, process, formats, then the box's colours, main first.
type Seed = (&'static str, &'static str, u32, &'static str, &'static str, &'static [&'static str], &'static [&'static str]);

const NEG: &str = "colourNegative";
const SLIDE: &str = "slide";
const BW: &str = "blackAndWhite";

/// The makers' own colours, which most of their stocks share.
const KODAK: &str = "#f7c600";
const FUJIFILM: &str = "#2e9e4f";
const BLACK: &str = "#1c1c1c";
const BLUE: &str = "#2f6db5";
const RED: &str = "#d7262f";
const MAGENTA: &str = "#c2187a";
const PORTRA: &[&str] = &["#ecb445", "#7174b5", "#793a8e"];
const TRI_X: &[&str] = &[KODAK, "#3a6c57"];
const T_MAX: &[&str] = &[KODAK, "#9ca2b3"];

const ALL: &[&str] = &["35mm", "120", "Sheet"];
const ROLL: &[&str] = &["35mm", "120"];
const SMALL: &[&str] = &["35mm"];
const CINE: &[&str] = &["35mm", "16mm"];

#[rustfmt::skip]
const SEEDS: &[Seed] = &[
    ("Kodak", "Portra 160", 160, NEG, "C-41", ALL, PORTRA),
    ("Kodak", "Portra 400", 400, NEG, "C-41", ALL, PORTRA),
    ("Kodak", "Portra 800", 800, NEG, "C-41", ROLL, PORTRA),
    ("Kodak", "Ektar 100", 100, NEG, "C-41", ALL, &[KODAK, BLACK, "#8a332e"]),
    ("Kodak", "Gold 200", 200, NEG, "C-41", ROLL, &[KODAK, "#7f176b"]),
    ("Kodak", "UltraMax 400", 400, NEG, "C-41", SMALL, &[KODAK, BLUE]),
    ("Kodak", "ColorPlus 200", 200, NEG, "C-41", SMALL, &[KODAK, RED]),
    ("Kodak", "Pro Image 100", 100, NEG, "C-41", SMALL, &[KODAK, BLACK]),
    ("Kodak", "Ektachrome E100", 100, SLIDE, "E-6", ALL, &[KODAK, "#283d8f"]),
    ("Kodak", "Tri-X 400", 400, BW, "B&W", ROLL, TRI_X),
    ("Kodak", "Tri-X 320", 320, BW, "B&W", &["Sheet"], TRI_X),
    ("Kodak", "T-Max 100", 100, BW, "B&W", ALL, T_MAX),
    ("Kodak", "T-Max 400", 400, BW, "B&W", ALL, T_MAX),
    ("Kodak", "T-Max P3200", 3200, BW, "B&W", SMALL, T_MAX),
    ("Kodak", "Double-X 5222", 250, BW, "B&W", CINE, &[KODAK, BLACK]),
    ("Kodak", "Vision3 50D", 50, NEG, "ECN-2", CINE, &[KODAK, BLACK]),
    ("Kodak", "Vision3 250D", 250, NEG, "ECN-2", CINE, &[KODAK, BLACK]),
    ("Kodak", "Vision3 200T", 200, NEG, "ECN-2", CINE, &[KODAK, BLACK]),
    ("Kodak", "Vision3 500T", 500, NEG, "ECN-2", CINE, &[KODAK, BLACK]),
    ("Fujifilm", "200", 200, NEG, "C-41", ROLL, &[FUJIFILM, MAGENTA]),
    ("Fujifilm", "400", 400, NEG, "C-41", SMALL, &[FUJIFILM, "#4b86b9"]),
    ("Fujifilm", "C200", 200, NEG, "C-41", SMALL, &[FUJIFILM, MAGENTA]),
    ("Fujifilm", "Superia X-TRA 400", 400, NEG, "C-41", SMALL, &[FUJIFILM, "#1b4397"]),
    ("Fujifilm", "Pro 400H", 400, NEG, "C-41", ROLL, &[FUJIFILM, BLACK, "#642f88"]),
    ("Fujifilm", "Provia 100F", 100, SLIDE, "E-6", ALL, &[FUJIFILM, BLACK, "#3b40ed"]),
    ("Fujifilm", "Velvia 50", 50, SLIDE, "E-6", ALL, &[FUJIFILM, BLACK, BLUE]),
    ("Fujifilm", "Velvia 100", 100, SLIDE, "E-6", ALL, &[FUJIFILM, BLACK, BLUE]),
    ("Fujifilm", "Acros 100 II", 100, BW, "B&W", ROLL, &[FUJIFILM, BLACK]),
    ("CineStill", "800T", 800, NEG, "C-41", ALL, &[RED]),
    ("CineStill", "50D", 50, NEG, "C-41", ALL, &[BLUE]),
    ("CineStill", "400D", 400, NEG, "C-41", ROLL, &["#8f84de"]),
    ("CineStill", "BwXX", 250, BW, "B&W", SMALL, &["#8a8a8a"]),
];

pub fn seeds() -> Vec<StockFields> {
    SEEDS
        .iter()
        .map(|&(maker, name, iso, kind, process, formats, colours)| StockFields {
            maker: maker.into(),
            name: name.into(),
            box_iso: Some(iso),
            kind: kind.into(),
            process: process.into(),
            formats: formats.iter().map(|f| f.to_string()).collect(),
            colour: colours.first().map(|c| c.to_string()),
            colour2: colours.get(1).map(|c| c.to_string()),
            colour3: colours.get(2).map(|c| c.to_string()),
        })
        .collect()
}

impl Catalog {
    /// Adds the seeded stocks the first time a catalog is opened, and never
    /// again, so a stock deleted from the list stays deleted.
    pub(super) fn seed_film_stocks(&self) -> Result<(), String> {
        let seeded: bool = self
            .conn
            .query_row("SELECT EXISTS(SELECT 1 FROM meta WHERE key = 'stocks_seeded')", [], |r| r.get(0))
            .map_err(err)?;
        if seeded {
            return Ok(());
        }
        for stock in seeds() {
            self.create_film_stock(&stock)?;
        }
        self.conn
            .execute("INSERT INTO meta (key, value) VALUES ('stocks_seeded', '1')", [])
            .map(|_| ())
            .map_err(err)
    }

    pub fn film_stocks(&self) -> Result<Vec<FilmStock>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, maker, name, box_iso, kind, process, formats, colour, colour2, colour3
                 FROM film_stocks ORDER BY maker COLLATE NOCASE, id",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                let formats: String = r.get(6)?;
                Ok(FilmStock {
                    id: r.get(0)?,
                    fields: StockFields {
                        maker: r.get(1)?,
                        name: r.get(2)?,
                        box_iso: r.get(3)?,
                        kind: r.get(4)?,
                        process: r.get(5)?,
                        formats: serde_json::from_str(&formats).unwrap_or_default(),
                        colour: r.get(7)?,
                        colour2: r.get(8)?,
                        colour3: r.get(9)?,
                    },
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    pub fn create_film_stock(&self, stock: &StockFields) -> Result<FilmStock, String> {
        self.conn
            .execute(
                "INSERT INTO film_stocks (maker, name, box_iso, kind, process, formats, colour, colour2, colour3)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    stock.maker,
                    stock.name,
                    stock.box_iso,
                    stock.kind,
                    stock.process,
                    serde_json::to_string(&stock.formats).map_err(err)?,
                    stock.colour,
                    stock.colour2,
                    stock.colour3,
                ],
            )
            .map_err(err)?;
        Ok(FilmStock { id: self.conn.last_insert_rowid(), fields: stock.clone() })
    }

    pub fn update_film_stock(&self, id: i64, stock: &StockFields) -> Result<(), String> {
        let changed = self
            .conn
            .execute(
                "UPDATE film_stocks SET maker = ?1, name = ?2, box_iso = ?3, kind = ?4, process = ?5,
                 formats = ?6, colour = ?7, colour2 = ?8, colour3 = ?9 WHERE id = ?10",
                params![
                    stock.maker,
                    stock.name,
                    stock.box_iso,
                    stock.kind,
                    stock.process,
                    serde_json::to_string(&stock.formats).map_err(err)?,
                    stock.colour,
                    stock.colour2,
                    stock.colour3,
                    id,
                ],
            )
            .map_err(err)?;
        if changed == 0 {
            return Err(format!("No film stock {id} in the catalog"));
        }
        Ok(())
    }

    /// Removes a stock. The rolls shot on it keep its name in their information.
    pub fn delete_film_stock(&self, id: i64) -> Result<(), String> {
        let label = self
            .film_stocks()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("No film stock {id} in the catalog"))?
            .fields
            .label();
        let rolls: Vec<(i64, String)> = {
            let mut stmt = self.conn.prepare("SELECT id, info FROM rolls WHERE stock_id = ?1").map_err(err)?;
            let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?;
            rows.collect::<Result<_, _>>().map_err(err)?
        };
        for (roll, info) in rolls {
            let mut info: Value = serde_json::from_str(&info).unwrap_or(Value::Object(Default::default()));
            if let Value::Object(fields) = &mut info {
                fields.insert("filmStock".into(), Value::String(label.clone()));
            }
            self.conn
                .execute("UPDATE rolls SET info = ?1 WHERE id = ?2", params![info.to_string(), roll])
                .map_err(err)?;
        }
        self.conn.execute("DELETE FROM film_stocks WHERE id = ?1", params![id]).map(|_| ()).map_err(err)
    }
}
