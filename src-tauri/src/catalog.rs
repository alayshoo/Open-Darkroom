// src-tauri/src/catalog.rs
//
// The record of every library, roll and image on this computer, kept in one
// SQLite file in the app's data directory. See docs/library.md.
//
// Free of Tauri, so the tests can drive it against a temporary directory; the
// commands that expose it are in catalog_commands.rs.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::metadata::{self, ImageMetadata};

mod film_stocks;
mod gear;
mod moved_files;
pub use film_stocks::{FilmStock, StockFields};
pub use gear::{Gear, GearDetails, GearFields, RollGear};
pub use moved_files::{find_moved, search_roots, MissingFile};

/// How many stocks a new catalog starts with.
pub fn film_stocks_seeds_len() -> usize {
    film_stocks::seeds().len()
}

pub const CATALOG_FILE: &str = "catalog.sqlite";
pub const THUMBNAIL_DIR: &str = "thumbnails";

/// Bytes read from each of the start, middle and end of a file for its fingerprint.
const FINGERPRINT_BYTES: u64 = 64 * 1024;

/// Starts every fingerprint, telling it from the older kind read off the start of a file alone.
const FINGERPRINT_PREFIX: &str = "s2-";

const SCHEMA_VERSION: i64 = 5;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS libraries (
    id      INTEGER PRIMARY KEY,
    name    TEXT NOT NULL,
    created INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS film_stocks (
    id        INTEGER PRIMARY KEY,
    maker     TEXT NOT NULL,
    name      TEXT NOT NULL,
    box_iso   INTEGER,
    kind      TEXT NOT NULL,
    process   TEXT NOT NULL,
    formats   TEXT NOT NULL DEFAULT '[]',
    colour    TEXT,
    colour2   TEXT,
    colour3   TEXT
);
CREATE TABLE IF NOT EXISTS gear (
    id      INTEGER PRIMARY KEY,
    kind    TEXT NOT NULL,
    maker   TEXT NOT NULL,
    name    TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS rolls (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    info       TEXT NOT NULL DEFAULT '{}',
    created    INTEGER NOT NULL,
    stock_id   INTEGER REFERENCES film_stocks(id) ON DELETE SET NULL,
    gear       TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS images (
    id              INTEGER PRIMARY KEY,
    library_id      INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    roll_id         INTEGER REFERENCES rolls(id) ON DELETE SET NULL,
    frame           INTEGER,
    path            TEXT NOT NULL,
    in_photo_folder INTEGER NOT NULL DEFAULT 0,
    file_size       INTEGER NOT NULL,
    fingerprint     TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    frame_info      TEXT NOT NULL DEFAULT '{}',
    edit            TEXT,
    history         TEXT,
    edited          INTEGER,
    added           INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS images_library ON images(library_id);
CREATE INDEX IF NOT EXISTS images_roll ON images(roll_id);
";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub id: i64,
    pub name: String,
    pub created: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Roll {
    pub id: i64,
    pub library_id: i64,
    pub name: String,
    /// Film, camera and development, as the frontend's roll form writes it.
    pub info: Value,
    pub created: i64,
    /// The stock it was shot on, when it is one the catalog knows.
    pub stock_id: Option<i64>,
    /// The gear it was shot, developed and scanned with, of what the catalog knows.
    pub gear: RollGear,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRecord {
    pub id: i64,
    pub library_id: i64,
    pub roll_id: Option<i64>,
    pub frame: Option<i64>,
    pub file_name: String,
    /// Where the file is now, resolved against the photo folder if it is in it.
    pub path: String,
    pub in_photo_folder: bool,
    /// The file is not where the catalog last saw it.
    pub missing: bool,
    pub metadata: ImageMetadata,
    pub frame_info: Value,
    /// When its edit last changed, or None while it has never been edited.
    pub edited: Option<i64>,
    pub added: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub libraries: Vec<Library>,
    pub rolls: Vec<Roll>,
    pub images: Vec<ImageRecord>,
    pub film_stocks: Vec<FilmStock>,
    pub gear: Vec<Gear>,
    pub photo_folder: Option<String>,
}

/// What an import added, and the files it skipped.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub images: Vec<ImageRecord>,
    pub skipped: Vec<Skipped>,
}

/// A file an import could not read or copy, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

/// Why a file was not added: a problem with the file skips it, any other stops the import.
enum Failure {
    File(String),
    Fatal(String),
}

/// Where an import or a move puts images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub library_id: i64,
    pub roll_id: Option<i64>,
}

/// Where an import puts what it adds: a place that is there, or a roll it makes.
#[derive(Debug, Clone)]
pub enum Target {
    Place(Place),
    NewRoll { library_id: i64, name: String },
}

/// What an import needs from the catalog before any file is read.
pub struct ImportPlan {
    target: Target,
    /// The photo folder, and the folder under it that files are copied into.
    destination: Option<(PathBuf, PathBuf)>,
    known: HashSet<String>,
}

/// A file read, and copied when the import copies, ready to be recorded.
struct Prepared {
    stored: String,
    in_photo_folder: bool,
    size: u64,
    print: String,
    metadata: String,
    copy: Option<PathBuf>,
}

/// The files of an import, read and copied without the catalog.
pub struct PreparedImport {
    target: Target,
    files: Vec<Prepared>,
    skipped: Vec<Skipped>,
}

impl PreparedImport {
    /// Removes the copies made for an import that will not be recorded.
    fn discard(&self) {
        for copy in self.files.iter().filter_map(|f| f.copy.as_ref()) {
            let _ = fs::remove_file(copy);
        }
    }
}

/// Reads each file of an import, and copies it when the plan says to. Needs no
/// catalog, so it can run while the catalog is free.
pub fn prepare_import(plan: ImportPlan, paths: &[PathBuf]) -> Result<PreparedImport, String> {
    let ImportPlan { target, destination, mut known } = plan;
    let mut prepared = PreparedImport { target, files: Vec::new(), skipped: Vec::new() };
    let mut folder_made = false;
    for path in paths {
        match prepare_file(path, destination.as_ref(), &mut known, &mut folder_made) {
            Ok(Some(file)) => prepared.files.push(file),
            Ok(None) => {}
            Err(Failure::File(reason)) => prepared.skipped.push(Skipped { path: path.to_string_lossy().into_owned(), reason }),
            Err(Failure::Fatal(e)) => {
                prepared.discard();
                return Err(e);
            }
        }
    }
    Ok(prepared)
}

/// One file of an import, or None when the catalog or this import already has it.
fn prepare_file(
    path: &Path,
    destination: Option<&(PathBuf, PathBuf)>,
    known: &mut HashSet<String>,
    folder_made: &mut bool,
) -> Result<Option<Prepared>, Failure> {
    let (size, print) = fingerprint(path).map_err(Failure::File)?;
    if known.contains(&print) {
        return Ok(None);
    }
    let metadata = serde_json::to_string(&metadata::read(path)).map_err(|e| Failure::Fatal(err(e)))?;
    let (stored, in_photo_folder, copy) = match destination {
        Some((root, sub)) => {
            let folder = root.join(sub);
            if !*folder_made {
                fs::create_dir_all(&folder).map_err(|e| Failure::Fatal(format!("Could not make {}: {e}", folder.display())))?;
                *folder_made = true;
            }
            let name = path.file_name().ok_or_else(|| Failure::File("It has no name".into()))?.to_string_lossy();
            let target = reserve_path(&folder, &name).map_err(|e| Failure::File(format!("Could not copy it: {e}")))?;
            fs::copy(path, &target).map_err(|e| {
                let _ = fs::remove_file(&target);
                Failure::File(format!("Could not copy it: {e}"))
            })?;
            let stored = target.file_name().map(|n| sub.join(n)).unwrap_or_default();
            (stored.to_string_lossy().into_owned(), true, Some(target))
        }
        None => (absolute(path).to_string_lossy().into_owned(), false, None),
    };
    known.insert(print.clone());
    Ok(Some(Prepared { stored, in_photo_folder, size, print, metadata, copy }))
}

pub struct Catalog {
    conn: Connection,
    dir: PathBuf,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A file's size and an FNV-1a hash of its start, middle and end: enough to
/// know it again after it has moved, without reading all of it.
pub fn fingerprint(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(err)?;
    let size = file.metadata().map_err(err)?.len();
    let spans = if size <= 3 * FINGERPRINT_BYTES {
        vec![(0, size)]
    } else {
        vec![(0, FINGERPRINT_BYTES), ((size - FINGERPRINT_BYTES) / 2, FINGERPRINT_BYTES), (size - FINGERPRINT_BYTES, FINGERPRINT_BYTES)]
    };
    let mut hash = FNV_OFFSET;
    for (start, len) in spans {
        file.seek(SeekFrom::Start(start)).map_err(err)?;
        hash = fnv(hash, &read_span(&mut file, len)?);
    }
    Ok((size, format!("{FINGERPRINT_PREFIX}{size:x}-{hash:016x}")))
}

/// The fingerprint earlier builds took, of the start of a file alone.
fn legacy_fingerprint(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(err)?;
    let size = file.metadata().map_err(err)?.len();
    let hash = fnv(FNV_OFFSET, &read_span(&mut file, FINGERPRINT_BYTES)?);
    Ok(format!("{size:x}-{hash:016x}"))
}

/// The fingerprint of `path` of the same kind as `stored`, to compare with it.
pub fn fingerprint_like(stored: &str, path: &Path) -> Result<String, String> {
    if stored.starts_with(FINGERPRINT_PREFIX) { fingerprint(path).map(|(_, print)| print) } else { legacy_fingerprint(path) }
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn read_span(file: &mut fs::File, len: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(len as usize);
    file.take(len).read_to_end(&mut bytes).map_err(err)?;
    Ok(bytes)
}

/// Whether a path names a file the app opens.
pub fn is_image_file(path: &Path, extensions: &[String]) -> bool {
    path.is_file()
        && path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .is_some_and(|e| extensions.iter().any(|x| *x == e))
}

/// The images directly inside a folder, by name.
pub fn folder_images(folder: &Path, extensions: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = fs::read_dir(folder)
        .map_err(err)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_image_file(p, extensions))
        .collect();
    paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    Ok(paths)
}

/// A name made safe to use as one folder level.
fn folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() { "Untitled".into() } else { cleaned }
}

/// Makes an empty `dir/name`, or `dir/stem (2).ext` and so on if that is
/// taken, so no other import can take the same name before it is written.
fn reserve_path(dir: &Path, name: &str) -> std::io::Result<PathBuf> {
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let names = std::iter::once(dir.join(name)).chain((2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))));
    for candidate in names {
        match fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("the names never run out")
}

impl Catalog {
    /// Opens the catalog in `dir`, making it if there is none yet.
    pub fn open(dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(dir.join(THUMBNAIL_DIR)).map_err(err)?;
        let conn = Connection::open(dir.join(CATALOG_FILE)).map_err(err)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;").map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        let catalog = Self { conn, dir: dir.to_path_buf() };
        catalog.migrate()?;
        catalog.seed_film_stocks()?;
        Ok(catalog)
    }

    /// Brings a catalog written by an older build up to this one's schema.
    fn migrate(&self) -> Result<(), String> {
        let columns = [
            ("film_stocks", "colour3", "TEXT"),
            ("rolls", "gear", "TEXT NOT NULL DEFAULT '{}'"),
            ("rolls", "stock_id", "INTEGER REFERENCES film_stocks(id) ON DELETE SET NULL"),
            ("images", "history", "TEXT"),
        ];
        for (table, column, definition) in columns {
            let has: i64 = self
                .conn
                .query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1"), params![column], |r| {
                    r.get(0)
                })
                .map_err(err)?;
            if has == 0 {
                self.conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition};")).map_err(err)?;
            }
        }
        self.refingerprint()?;
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES ('schema', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![SCHEMA_VERSION.to_string()],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// Takes fingerprints of the current kind for images that still have the
    /// older kind and whose files are there. Missing ones keep theirs to be found by.
    fn refingerprint(&self) -> Result<(), String> {
        let photo_folder = self.photo_folder()?;
        let old: Vec<(i64, PathBuf)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, path, in_photo_folder FROM images WHERE fingerprint NOT LIKE ?1")
                .map_err(err)?;
            let rows = stmt
                .query_map(params![format!("{FINGERPRINT_PREFIX}%")], |r| {
                    Ok((r.get(0)?, resolve(&r.get::<_, String>(1)?, r.get(2)?, photo_folder.as_deref())))
                })
                .map_err(err)?;
            rows.collect::<Result<_, _>>().map_err(err)?
        };
        let fresh: Vec<(i64, String)> =
            old.into_iter().filter_map(|(id, path)| fingerprint(&path).ok().map(|(_, print)| (id, print))).collect();
        if fresh.is_empty() {
            return Ok(());
        }
        self.atomically(|| {
            for (id, print) in &fresh {
                self.conn.execute("UPDATE images SET fingerprint = ?1 WHERE id = ?2", params![print, id]).map_err(err)?;
            }
            Ok(())
        })
    }

    pub fn thumbnail_path(&self, image_id: i64) -> PathBuf {
        self.dir.join(THUMBNAIL_DIR).join(format!("{image_id}.jpg"))
    }

    // ── Photo folder ─────────────────────────────────────────────────────────

    pub fn photo_folder(&self) -> Result<Option<PathBuf>, String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = 'photo_folder'", [], |r| r.get::<_, String>(0))
            .optional()
            .map(|v| v.map(PathBuf::from))
            .map_err(err)
    }

    pub fn set_photo_folder(&self, folder: &Path) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES ('photo_folder', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![absolute(folder).to_string_lossy()],
            )
            .map(|_| ())
            .map_err(err)
    }

    // ── Reading ──────────────────────────────────────────────────────────────

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let photo_folder = self.photo_folder()?;
        Ok(Snapshot {
            libraries: self.libraries()?,
            rolls: self.rolls()?,
            images: self.images("", [])?,
            film_stocks: self.film_stocks()?,
            gear: self.gear()?,
            photo_folder: photo_folder.map(|p| p.to_string_lossy().into_owned()),
        })
    }

    pub fn libraries(&self) -> Result<Vec<Library>, String> {
        let mut stmt = self.conn.prepare("SELECT id, name, created FROM libraries ORDER BY created, id").map_err(err)?;
        let rows = stmt
            .query_map([], |r| Ok(Library { id: r.get(0)?, name: r.get(1)?, created: r.get(2)? }))
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    pub fn rolls(&self) -> Result<Vec<Roll>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, library_id, name, info, created, stock_id, gear FROM rolls ORDER BY created, id")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                let info: String = r.get(3)?;
                let gear: String = r.get(6)?;
                Ok(Roll {
                    id: r.get(0)?,
                    library_id: r.get(1)?,
                    name: r.get(2)?,
                    info: serde_json::from_str(&info).unwrap_or(Value::Object(Default::default())),
                    created: r.get(4)?,
                    stock_id: r.get(5)?,
                    gear: serde_json::from_str(&gear).unwrap_or_default(),
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    fn images<P: rusqlite::Params>(&self, filter: &str, params: P) -> Result<Vec<ImageRecord>, String> {
        let photo_folder = self.photo_folder()?;
        let sql = format!(
            "SELECT id, library_id, roll_id, frame, path, in_photo_folder, metadata, frame_info, edited, added
             FROM images {filter} ORDER BY roll_id, frame, added, id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(err)?;
        let rows = stmt
            .query_map(params, |r| {
                let stored: String = r.get(4)?;
                let in_photo_folder: bool = r.get(5)?;
                let metadata: String = r.get(6)?;
                let frame_info: String = r.get(7)?;
                Ok((
                    ImageRecord {
                        id: r.get(0)?,
                        library_id: r.get(1)?,
                        roll_id: r.get(2)?,
                        frame: r.get(3)?,
                        file_name: String::new(),
                        path: String::new(),
                        in_photo_folder,
                        missing: false,
                        metadata: serde_json::from_str(&metadata).unwrap_or_default(),
                        frame_info: serde_json::from_str(&frame_info).unwrap_or(Value::Object(Default::default())),
                        edited: r.get(8)?,
                        added: r.get(9)?,
                    },
                    stored,
                ))
            })
            .map_err(err)?;
        rows.map(|row| {
            let (mut image, stored) = row.map_err(err)?;
            let path = resolve(&stored, image.in_photo_folder, photo_folder.as_deref());
            image.file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            image.missing = !path.is_file();
            image.path = path.to_string_lossy().into_owned();
            Ok(image)
        })
        .collect()
    }

    pub fn image(&self, id: i64) -> Result<ImageRecord, String> {
        self.images("WHERE id = ?1", params![id])?.pop().ok_or_else(|| format!("No image {id} in the catalog"))
    }

    // ── Libraries and rolls ──────────────────────────────────────────────────

    pub fn create_library(&self, name: &str) -> Result<Library, String> {
        let created = now();
        self.conn.execute("INSERT INTO libraries (name, created) VALUES (?1, ?2)", params![name, created]).map_err(err)?;
        Ok(Library { id: self.conn.last_insert_rowid(), name: name.into(), created })
    }

    pub fn rename_library(&self, id: i64, name: &str) -> Result<(), String> {
        self.conn.execute("UPDATE libraries SET name = ?1 WHERE id = ?2", params![name, id]).map(|_| ()).map_err(err)
    }

    /// Takes the library's rolls and image records with it. The files stay.
    pub fn delete_library(&self, id: i64) -> Result<(), String> {
        let ids = self.ids("SELECT id FROM images WHERE library_id = ?1", id)?;
        self.conn.execute("DELETE FROM libraries WHERE id = ?1", params![id]).map_err(err)?;
        self.forget_thumbnails(&ids);
        Ok(())
    }

    pub fn create_roll(&self, library_id: i64, name: &str) -> Result<Roll, String> {
        let created = now();
        self.conn
            .execute(
                "INSERT INTO rolls (library_id, name, created) VALUES (?1, ?2, ?3)",
                params![library_id, name, created],
            )
            .map_err(err)?;
        Ok(Roll {
            id: self.conn.last_insert_rowid(),
            library_id,
            name: name.into(),
            info: Value::Object(Default::default()),
            created,
            stock_id: None,
            gear: RollGear::default(),
        })
    }

    pub fn update_roll(&self, id: i64, name: &str, info: &Value, stock_id: Option<i64>, gear: &RollGear) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE rolls SET name = ?1, info = ?2, stock_id = ?3, gear = ?4 WHERE id = ?5",
                params![name, info.to_string(), stock_id, serde_json::to_string(gear).map_err(err)?, id],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// The roll's frames stay in its library, loose.
    pub fn delete_roll(&self, id: i64) -> Result<(), String> {
        self.atomically(|| {
            self.conn.execute("UPDATE images SET frame = NULL WHERE roll_id = ?1", params![id]).map_err(err)?;
            self.conn.execute("DELETE FROM rolls WHERE id = ?1", params![id]).map(|_| ()).map_err(err)
        })
    }

    fn roll_library(&self, roll_id: i64) -> Result<i64, String> {
        self.conn
            .query_row("SELECT library_id FROM rolls WHERE id = ?1", params![roll_id], |r| r.get(0))
            .map_err(|_| format!("No roll {roll_id} in the catalog"))
    }

    /// A place made consistent: a roll always takes its own library.
    fn check_place(&self, place: Place) -> Result<Place, String> {
        match place.roll_id {
            Some(roll_id) => Ok(Place { library_id: self.roll_library(roll_id)?, roll_id: Some(roll_id) }),
            None => {
                let exists: bool = self
                    .conn
                    .query_row("SELECT EXISTS(SELECT 1 FROM libraries WHERE id = ?1)", params![place.library_id], |r| {
                        r.get(0)
                    })
                    .map_err(err)?;
                if exists { Ok(place) } else { Err(format!("No library {} in the catalog", place.library_id)) }
            }
        }
    }

    fn next_frame(&self, roll_id: i64) -> Result<i64, String> {
        self.conn
            .query_row("SELECT COALESCE(MAX(frame), 0) + 1 FROM images WHERE roll_id = ?1", params![roll_id], |r| r.get(0))
            .map_err(err)
    }

    // ── Images ───────────────────────────────────────────────────────────────

    /// Adds files to a library or roll, in the order given, copying them into
    /// the photo folder first with `copy`. A file already in the catalog is left
    /// out, one that cannot be read or copied is skipped, and any other failure adds nothing.
    pub fn import(&self, paths: &[PathBuf], place: Place, copy: bool) -> Result<Import, String> {
        let plan = self.plan_import(Target::Place(place), copy)?;
        self.record_import(prepare_import(plan, paths)?).map(|(_, import)| import)
    }

    /// Makes a roll of `paths`, numbered in the order given. There is no roll
    /// when none of them was added.
    pub fn import_roll(&self, library_id: i64, name: &str, paths: &[PathBuf], copy: bool) -> Result<(Option<Roll>, Import), String> {
        let plan = self.plan_import(Target::NewRoll { library_id, name: name.into() }, copy)?;
        self.record_import(prepare_import(plan, paths)?)
    }

    /// What an import needs from the catalog before any file is read.
    pub fn plan_import(&self, target: Target, copy: bool) -> Result<ImportPlan, String> {
        let target = match target {
            Target::Place(place) => Target::Place(self.check_place(place)?),
            Target::NewRoll { library_id, name } => {
                self.check_place(Place { library_id, roll_id: None })?;
                Target::NewRoll { library_id, name }
            }
        };
        let destination = if copy { Some(self.copy_destination(&target)?) } else { None };
        Ok(ImportPlan { target, destination, known: self.known_fingerprints()? })
    }

    /// Records the files `prepare_import` read, as one transaction. A file the
    /// catalog got meanwhile is left out, and every copy is removed on failure.
    pub fn record_import(&self, prepared: PreparedImport) -> Result<(Option<Roll>, Import), String> {
        let recorded = self.atomically(|| self.record_files(&prepared));
        if recorded.is_err() {
            prepared.discard();
        }
        recorded
    }

    fn record_files(&self, prepared: &PreparedImport) -> Result<(Option<Roll>, Import), String> {
        let known = self.known_fingerprints()?;
        let (fresh, taken): (Vec<&Prepared>, Vec<&Prepared>) = prepared.files.iter().partition(|f| !known.contains(&f.print));
        for copy in taken.iter().filter_map(|f| f.copy.as_ref()) {
            let _ = fs::remove_file(copy);
        }
        let mut import = Import { images: Vec::new(), skipped: prepared.skipped.clone() };
        let (place, roll) = match &prepared.target {
            // The place may have gone while the files were read.
            Target::Place(place) => (self.check_place(*place)?, None),
            Target::NewRoll { library_id, name } => {
                if fresh.is_empty() {
                    return Ok((None, import));
                }
                let roll = self.create_roll(*library_id, name)?;
                (Place { library_id: *library_id, roll_id: Some(roll.id) }, Some(roll))
            }
        };
        for file in fresh {
            let frame = match place.roll_id {
                Some(roll_id) => Some(self.next_frame(roll_id)?),
                None => None,
            };
            self.conn
                .execute(
                    "INSERT INTO images (library_id, roll_id, frame, path, in_photo_folder, file_size, fingerprint, metadata, added)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        place.library_id,
                        place.roll_id,
                        frame,
                        file.stored,
                        file.in_photo_folder,
                        file.size as i64,
                        file.print,
                        file.metadata,
                        now()
                    ],
                )
                .map_err(err)?;
            import.images.push(self.image(self.conn.last_insert_rowid())?);
        }
        Ok((roll, import))
    }

    /// Runs `f` as one transaction, undone if it fails.
    fn atomically<T>(&self, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let transaction = self.conn.unchecked_transaction().map_err(err)?;
        let out = f()?;
        transaction.commit().map_err(err)?;
        Ok(out)
    }

    /// The image already in the catalog for a file, if there is one.
    pub fn find_by_file(&self, path: &Path) -> Result<Option<i64>, String> {
        self.find_by_fingerprint(&fingerprint(path)?.1)
    }

    pub fn find_by_fingerprint(&self, print: &str) -> Result<Option<i64>, String> {
        self.conn
            .query_row("SELECT id FROM images WHERE fingerprint = ?1", params![print], |r| r.get(0))
            .optional()
            .map_err(err)
    }

    fn known_fingerprints(&self) -> Result<HashSet<String>, String> {
        let mut stmt = self.conn.prepare("SELECT fingerprint FROM images").map_err(err)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    /// The photo folder, and the folder under it for an import: one per
    /// library, and one per roll inside that.
    fn copy_destination(&self, target: &Target) -> Result<(PathBuf, PathBuf), String> {
        let root = self.photo_folder()?.ok_or("Choose a photo folder in Settings before copying images into it")?;
        let (library_id, roll) = match target {
            Target::Place(place) => {
                let roll = match place.roll_id {
                    Some(roll_id) => Some(
                        self.conn
                            .query_row("SELECT name FROM rolls WHERE id = ?1", params![roll_id], |r| r.get::<_, String>(0))
                            .map_err(err)?,
                    ),
                    None => None,
                };
                (place.library_id, roll)
            }
            Target::NewRoll { library_id, name } => (*library_id, Some(name.clone())),
        };
        let library: String = self
            .conn
            .query_row("SELECT name FROM libraries WHERE id = ?1", params![library_id], |r| r.get(0))
            .map_err(err)?;
        let mut sub = PathBuf::from(folder_name(&library));
        if let Some(roll) = roll {
            sub.push(folder_name(&roll));
        }
        Ok((root, sub))
    }

    /// Moves records only; files stay where they are. Images moved onto a
    /// roll are numbered on after its last frame, in the order given.
    pub fn move_images(&self, ids: &[i64], place: Place) -> Result<Vec<ImageRecord>, String> {
        let place = self.check_place(place)?;
        self.atomically(|| {
            for &id in ids {
                let current: Option<i64> = self
                    .conn
                    .query_row("SELECT roll_id FROM images WHERE id = ?1", params![id], |r| r.get(0))
                    .map_err(err)?;
                if current.is_some() && current == place.roll_id {
                    continue;
                }
                let frame = match place.roll_id {
                    Some(roll_id) => Some(self.next_frame(roll_id)?),
                    None => None,
                };
                self.conn
                    .execute(
                        "UPDATE images SET library_id = ?1, roll_id = ?2, frame = ?3 WHERE id = ?4",
                        params![place.library_id, place.roll_id, frame, id],
                    )
                    .map_err(err)?;
            }
            ids.iter().map(|&id| self.image(id)).collect()
        })
    }

    /// Takes images out of the catalog. Their files are left alone.
    pub fn remove_images(&self, ids: &[i64]) -> Result<(), String> {
        self.atomically(|| {
            for &id in ids {
                self.conn.execute("DELETE FROM images WHERE id = ?1", params![id]).map_err(err)?;
            }
            Ok(())
        })?;
        self.forget_thumbnails(ids);
        Ok(())
    }

    pub fn set_frame_info(&self, id: i64, info: &Value) -> Result<(), String> {
        self.conn
            .execute("UPDATE images SET frame_info = ?1 WHERE id = ?2", params![info.to_string(), id])
            .map(|_| ())
            .map_err(err)
    }

    /// Points a missing image at a file, as long as it is the same file.
    pub fn relink(&self, id: i64, path: &Path) -> Result<ImageRecord, String> {
        let stored: String = self
            .conn
            .query_row("SELECT fingerprint FROM images WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(err)?;
        if fingerprint_like(&stored, path)? != stored {
            return Err("That is not the same file".into());
        }
        let (_, print) = fingerprint(path)?;
        let path = absolute(path);
        let root = self.photo_folder()?.map(|root| absolute(&root));
        let (stored, in_photo_folder) = match root {
            Some(root) if path.starts_with(&root) => {
                (path.strip_prefix(&root).map_err(err)?.to_string_lossy().into_owned(), true)
            }
            _ => (path.to_string_lossy().into_owned(), false),
        };
        self.conn
            .execute(
                "UPDATE images SET path = ?1, in_photo_folder = ?2, fingerprint = ?3 WHERE id = ?4",
                params![stored, in_photo_folder, print, id],
            )
            .map_err(err)?;
        self.image(id)
    }

    // ── Edits ────────────────────────────────────────────────────────────────

    /// The stored edit document, as the frontend wrote it.
    pub fn edit(&self, id: i64) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT edit FROM images WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(err)
    }

    /// The undo history stored with the edit, as the frontend wrote it.
    pub fn edit_history(&self, id: i64) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT history FROM images WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(err)
    }

    /// Stores an edit with the undo history that led to it, and returns when it was saved.
    pub fn save_edit(&self, id: i64, edit: &str, history: Option<&str>) -> Result<i64, String> {
        let edited = now();
        let changed = self
            .conn
            .execute(
                "UPDATE images SET edit = ?1, history = ?2, edited = ?3 WHERE id = ?4",
                params![edit, history, edited, id],
            )
            .map_err(err)?;
        if changed == 0 {
            return Err(format!("No image {id} in the catalog"));
        }
        Ok(edited)
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn ids(&self, sql: &str, param: i64) -> Result<Vec<i64>, String> {
        let mut stmt = self.conn.prepare(sql).map_err(err)?;
        let rows = stmt.query_map(params![param], |r| r.get(0)).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    fn forget_thumbnails(&self, ids: &[i64]) {
        for &id in ids {
            let _ = fs::remove_file(self.thumbnail_path(id));
        }
    }
}

fn resolve(stored: &str, in_photo_folder: bool, photo_folder: Option<&Path>) -> PathBuf {
    match (in_photo_folder, photo_folder) {
        (true, Some(root)) => root.join(stored),
        _ => PathBuf::from(stored),
    }
}

fn absolute(path: &Path) -> PathBuf {
    fs::canonicalize(path).map(strip_verbatim).unwrap_or_else(|_| path.to_path_buf())
}

/// Windows' canonical paths carry a `\\?\` prefix that nothing else expects.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}
