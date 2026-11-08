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
    /// `#rrggbb`.
    pub colour: Option<String>,
    pub colour2: Option<String>,
}

impl StockFields {
    /// How a roll names the stock: the maker, then the stock.
    pub fn label(&self) -> String {
        if self.maker.is_empty() { self.name.clone() } else { format!("{} {}", self.maker, self.name) }
    }
}

type Seed = (&'static str, &'static str, u32, &'static str, &'static str, &'static [&'static str], &'static str, Option<&'static str>);

const NEG: &str = "colourNegative";
const SLIDE: &str = "slide";
const BW: &str = "blackAndWhite";

/// The makers' own colours, which most of their stocks share.
const KODAK: &str = "#f7c600";
const FUJIFILM: &str = "#2e9e4f";

const ALL: &[&str] = &["35mm", "120", "Sheet"];
const ROLL: &[&str] = &["35mm", "120"];
const SMALL: &[&str] = &["35mm"];
const CINE: &[&str] = &["35mm", "16mm"];

#[rustfmt::skip]
const SEEDS: &[Seed] = &[
    ("Kodak", "Portra 160", 160, NEG, "C-41", ALL, KODAK, None),
    ("Kodak", "Portra 400", 400, NEG, "C-41", ALL, KODAK, None),
    ("Kodak", "Portra 800", 800, NEG, "C-41", ROLL, KODAK, None),
    ("Kodak", "Ektar 100", 100, NEG, "C-41", ALL, KODAK, None),
    ("Kodak", "Gold 200", 200, NEG, "C-41", ROLL, KODAK, Some("#c2187a")),
    ("Kodak", "UltraMax 400", 400, NEG, "C-41", SMALL, KODAK, Some("#2f6db5")),
    ("Kodak", "ColorPlus 200", 200, NEG, "C-41", SMALL, KODAK, None),
    ("Kodak", "Pro Image 100", 100, NEG, "C-41", SMALL, KODAK, None),
    ("Kodak", "Ektachrome E100", 100, SLIDE, "E-6", ALL, KODAK, None),
    ("Kodak", "Tri-X 400", 400, BW, "B&W", ROLL, KODAK, None),
    ("Kodak", "Tri-X 320", 320, BW, "B&W", &["Sheet"], KODAK, None),
    ("Kodak", "T-Max 100", 100, BW, "B&W", ALL, KODAK, None),
    ("Kodak", "T-Max 400", 400, BW, "B&W", ALL, KODAK, None),
    ("Kodak", "T-Max P3200", 3200, BW, "B&W", SMALL, KODAK, None),
    ("Kodak", "Double-X 5222", 250, BW, "B&W", CINE, KODAK, None),
    ("Kodak", "Vision3 50D", 50, NEG, "ECN-2", CINE, KODAK, None),
    ("Kodak", "Vision3 250D", 250, NEG, "ECN-2", CINE, KODAK, None),
    ("Kodak", "Vision3 200T", 200, NEG, "ECN-2", CINE, KODAK, None),
    ("Kodak", "Vision3 500T", 500, NEG, "ECN-2", CINE, KODAK, None),
    ("Fujifilm", "200", 200, NEG, "C-41", ROLL, FUJIFILM, Some("#c2187a")),
    ("Fujifilm", "400", 400, NEG, "C-41", SMALL, FUJIFILM, None),
    ("Fujifilm", "C200", 200, NEG, "C-41", SMALL, FUJIFILM, None),
    ("Fujifilm", "Superia X-TRA 400", 400, NEG, "C-41", SMALL, FUJIFILM, None),
    ("Fujifilm", "Pro 400H", 400, NEG, "C-41", ROLL, FUJIFILM, None),
    ("Fujifilm", "Provia 100F", 100, SLIDE, "E-6", ALL, FUJIFILM, None),
    ("Fujifilm", "Velvia 50", 50, SLIDE, "E-6", ALL, FUJIFILM, None),
    ("Fujifilm", "Velvia 100", 100, SLIDE, "E-6", ALL, FUJIFILM, None),
    ("Fujifilm", "Acros 100 II", 100, BW, "B&W", ROLL, FUJIFILM, None),
    ("CineStill", "800T", 800, NEG, "C-41", ALL, "#d7262f", None),
    ("CineStill", "50D", 50, NEG, "C-41", ALL, "#2f6db5", None),
    ("CineStill", "400D", 400, NEG, "C-41", ROLL, "#f28c28", None),
    ("CineStill", "BwXX", 250, BW, "B&W", SMALL, "#8a8a8a", None),
];

pub fn seeds() -> Vec<StockFields> {
    SEEDS
        .iter()
        .map(|&(maker, name, iso, kind, process, formats, colour, colour2)| StockFields {
            maker: maker.into(),
            name: name.into(),
            box_iso: Some(iso),
            kind: kind.into(),
            process: process.into(),
            formats: formats.iter().map(|f| f.to_string()).collect(),
            colour: Some(colour.into()),
            colour2: colour2.map(Into::into),
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
                "SELECT id, maker, name, box_iso, kind, process, formats, colour, colour2
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
                    },
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    pub fn create_film_stock(&self, stock: &StockFields) -> Result<FilmStock, String> {
        self.conn
            .execute(
                "INSERT INTO film_stocks (maker, name, box_iso, kind, process, formats, colour, colour2)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    stock.maker,
                    stock.name,
                    stock.box_iso,
                    stock.kind,
                    stock.process,
                    serde_json::to_string(&stock.formats).map_err(err)?,
                    stock.colour,
                    stock.colour2,
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
                 formats = ?6, colour = ?7, colour2 = ?8 WHERE id = ?9",
                params![
                    stock.maker,
                    stock.name,
                    stock.box_iso,
                    stock.kind,
                    stock.process,
                    serde_json::to_string(&stock.formats).map_err(err)?,
                    stock.colour,
                    stock.colour2,
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
