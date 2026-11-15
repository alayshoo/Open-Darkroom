// src-tauri/src/image_prefetch.rs
//
// Decodes the image selected in the library ahead of it being opened, on one
// background thread. Only the latest image asked for is kept, alongside the one
// open in the editor.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::SystemTime;

use crate::image_opening::{decode_and_prepare, DecodedImage};

struct Decoded {
    path: PathBuf,
    /// When the file was last changed as it was decoded, so an edit made to it since is not missed.
    modified: Option<SystemTime>,
    result: Result<DecodedImage, String>,
}

struct Slot {
    /// The image last asked for, until it is opened or let go.
    wanted: Option<PathBuf>,
    /// Being decoded now.
    running: Option<PathBuf>,
    /// Decoded, until it is opened or let go.
    done: Option<Decoded>,
    /// An open is waiting on the image being decoded now.
    awaited: bool,
    worker: bool,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot { wanted: None, running: None, done: None, awaited: false, worker: false });
static FINISHED: Condvar = Condvar::new();

fn lock() -> MutexGuard<'static, Slot> {
    SLOT.lock().unwrap_or_else(|e| e.into_inner())
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Starts decoding `path` in the background, in place of any image asked for before it.
pub fn prefetch(path: PathBuf) {
    let mut slot = lock();
    let kept = slot.awaited || slot.done.as_ref().is_some_and(|d| d.path == path);
    if !kept {
        slot.done = None;
    }
    slot.wanted = Some(path);
    if !slot.worker {
        slot.worker = true;
        std::thread::spawn(work);
    }
}

/// Lets go of everything asked for. An image being decoded still finishes, and is then dropped.
pub fn cancel() {
    let mut slot = lock();
    slot.wanted = None;
    slot.done = None;
}

/// The decoded image for `path`, waiting for it if it is being decoded now.
/// Anything else asked for is let go, as the image being opened is the one wanted.
pub fn take(path: &Path) -> Option<DecodedImage> {
    let mut slot = lock();
    slot.wanted = None;
    while slot.running.as_deref() == Some(path) {
        slot.awaited = true;
        slot = FINISHED.wait(slot).unwrap_or_else(|e| e.into_inner());
    }
    slot.awaited = false;
    let done = slot.done.take().filter(|d| d.path == path);
    drop(slot);

    let done = done?;
    if done.modified != modified(path) {
        log::debug!("{} changed after it was decoded ahead, so it is decoded again", path.display());
        return None;
    }
    done.result.map_err(|e| log::debug!("Decoding {} ahead failed: {e}", path.display())).ok()
}

fn work() {
    loop {
        let path = {
            let mut slot = lock();
            let next = slot.wanted.clone().filter(|w| slot.done.as_ref().is_none_or(|d| &d.path != w));
            let Some(path) = next else {
                slot.worker = false;
                return;
            };
            slot.running = Some(path.clone());
            path
        };

        log::debug!("Decoding {} ahead", path.display());
        let modified = modified(&path);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| decode_and_prepare(&path)))
            .unwrap_or_else(|_| Err("Decoding panicked".to_string()));

        let mut slot = lock();
        slot.running = None;
        // Kept only while it is still wanted, so a superseded image does not sit beside the next.
        if slot.awaited || slot.wanted.as_ref() == Some(&path) {
            slot.done = Some(Decoded { path, modified, result });
        }
        FINISHED.notify_all();
    }
}

/// Decodes a library image ahead of it being opened, or with no path, lets go of what was.
#[tauri::command]
pub(crate) fn prefetch_image(path: Option<PathBuf>) {
    match path {
        Some(path) if path.is_file() => prefetch(path),
        _ => cancel(),
    }
}
