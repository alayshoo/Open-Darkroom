// src-tauri/src/catalog/moved_files.rs
//
// Finding files that have moved since the catalog last saw them, by their
// fingerprint, in the folders around where they were.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::{err, fingerprint, resolve, Catalog};

/// How deep a search goes below each folder it starts from.
const SEARCH_DEPTH: usize = 4;

/// The most folder entries one search looks at, so a search near the top of a
/// large drive still ends quickly.
const SEARCH_ENTRIES: usize = 50_000;

/// An image whose file is not where the catalog last saw it.
#[derive(Debug, Clone)]
pub struct MissingFile {
    pub id: i64,
    /// Where the file was.
    pub path: PathBuf,
    pub size: u64,
    pub fingerprint: String,
}

impl Catalog {
    pub fn missing_files(&self) -> Result<Vec<MissingFile>, String> {
        let photo_folder = self.photo_folder()?;
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, in_photo_folder, file_size, fingerprint FROM images ORDER BY id")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                let stored: String = r.get(1)?;
                let in_photo_folder: bool = r.get(2)?;
                Ok(MissingFile {
                    id: r.get(0)?,
                    path: resolve(&stored, in_photo_folder, photo_folder.as_deref()),
                    size: r.get::<_, i64>(3)? as u64,
                    fingerprint: r.get(4)?,
                })
            })
            .map_err(err)?;
        let all: Vec<MissingFile> = rows.collect::<Result<_, _>>().map_err(err)?;
        Ok(all.into_iter().filter(|m| !m.path.is_file()).collect())
    }
}

/// The folders to look in: the closest folders that are still there above
/// where each file was, then `near` and the photo folder.
pub fn search_roots(missing: &[MissingFile], near: Option<&Path>, photo_folder: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(near) = near {
        roots.push(near.to_path_buf());
        roots.extend(near.parent().filter(|p| p.parent().is_some()).map(Path::to_path_buf));
    }
    for file in missing {
        let Some(found) = file.path.ancestors().skip(1).find(|a| a.is_dir()) else { continue };
        roots.push(found.to_path_buf());
        // One level further up, unless that is the top of a drive.
        if let Some(parent) = found.parent().filter(|p| p.parent().is_some()) {
            roots.push(parent.to_path_buf());
        }
    }
    roots.extend(photo_folder.filter(|p| p.is_dir()).map(Path::to_path_buf));
    let mut seen = HashSet::new();
    roots.retain(|r| seen.insert(r.clone()));
    roots
}

/// Looks through `roots`, nearest folders first, for files with the same
/// fingerprint as a missing one. Only files of a missing one's size are read.
pub fn find_moved(missing: &[MissingFile], roots: &[PathBuf]) -> Vec<(i64, PathBuf)> {
    let mut by_size: HashMap<u64, Vec<&MissingFile>> = HashMap::new();
    for file in missing {
        by_size.entry(file.size).or_default().push(file);
    }
    let mut found: Vec<(i64, PathBuf)> = Vec::new();
    let mut claimed = HashSet::new();
    let mut visited = HashSet::new();
    let mut entries = 0;

    let mut queue: VecDeque<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        if found.len() == missing.len() || entries >= SEARCH_ENTRIES || !visited.insert(dir.clone()) {
            continue;
        }
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            entries += 1;
            if entries >= SEARCH_ENTRIES {
                break;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if depth < SEARCH_DEPTH {
                    queue.push_back((entry.path(), depth + 1));
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let Some(candidates) = entry.metadata().ok().and_then(|m| by_size.get(&m.len())) else { continue };
            let path = entry.path();
            let Ok((_, print)) = fingerprint(&path) else { continue };
            if let Some(file) = candidates.iter().find(|f| f.fingerprint == print && !claimed.contains(&f.id)) {
                claimed.insert(file.id);
                found.push((file.id, path));
            }
        }
    }
    found
}

