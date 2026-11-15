// src-tauri/src/image_prefetch.rs
//
// Decodes the image selected in the library ahead of it being opened, on one
// background thread. Only the latest image asked for is kept, alongside the one
// open in the editor.

use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::SystemTime;

use crate::image_opening::{decode_and_prepare_while, DecodedImage};

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

fn still_wanted(path: &Path) -> bool {
    let slot = lock();
    slot.awaited || slot.wanted.as_deref() == Some(path)
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
        // A superseded image stops before it is prepared, to hand the CPU and memory back to the next.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| decode_and_prepare_while(&path, || still_wanted(&path))))
            .unwrap_or_else(|_| Err("Decoding panicked".to_string()));

        let mut slot = lock();
        slot.running = None;
        // Kept only while it is still wanted, so a superseded image does not sit beside the next.
        let kept = slot.awaited || slot.wanted.as_ref() == Some(&path);
        match result.transpose() {
            Some(result) if kept => slot.done = Some(Decoded { path, modified, result }),
            None => log::debug!("Stopped decoding {} ahead, as another image was wanted", path.display()),
            _ => {}
        }
        FINISHED.notify_all();
    }
}

/// Decodes a library image ahead of it being opened, or with no path, lets go of what was.
/// It touches no files itself, as it runs on the main thread.
#[tauri::command]
pub(crate) fn prefetch_image(path: Option<PathBuf>) {
    match path {
        Some(path) => prefetch(path),
        None => cancel(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, shade: u8) {
        image::RgbImage::from_pixel(64, 48, image::Rgb([shade, shade, shade])).save(path).unwrap();
    }

    fn started() {
        while lock().running.is_none() && lock().done.is_none() {
            std::thread::yield_now();
        }
    }

    fn settled() {
        while lock().worker {
            std::thread::yield_now();
        }
    }

    // One test, as the slot is shared by the whole process.
    #[test]
    fn hands_over_only_the_image_still_wanted() {
        let dir = std::env::temp_dir().join(format!("prefetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.png"), dir.join("b.png"));
        write_png(&a, 40);
        write_png(&b, 200);

        prefetch(a.clone());
        started();
        let taken = take(&a).expect("decoded ahead");
        assert_eq!((taken.width, taken.height), (64, 48));
        assert!(lock().done.is_none());

        // Opening another image lets go of the one decoded ahead.
        prefetch(a.clone());
        settled();
        assert!(take(&b).is_none());
        assert!(lock().done.is_none() && lock().wanted.is_none());

        // A newer selection takes the place of the last.
        prefetch(a.clone());
        prefetch(b.clone());
        settled();
        assert_eq!(lock().done.as_ref().map(|d| d.path.clone()), Some(b.clone()));
        cancel();
        assert!(lock().done.is_none());

        // A file changed after it was decoded is decoded again on open.
        prefetch(a.clone());
        settled();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_png(&a, 90);
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&a).unwrap().set_modified(later).unwrap();
        assert!(take(&a).is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
