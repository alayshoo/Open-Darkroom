// src-tauri/tests/catalog.rs
//
// The catalog against real files in a scratch directory: libraries, rolls,
// importing by reference and by copy, edits, moving and finding files again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use image::{ImageBuffer, Rgb};
use serde_json::json;

use open_darkroom_lib::catalog::{fingerprint, folder_images, Catalog, Place};
use open_darkroom_lib::thumbnails::ThumbnailSource;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "open-darkroom-catalog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// A small PNG, different for each seed.
    fn image(&self, name: &str, seed: u8) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        ImageBuffer::from_fn(40, 30, |x, y| Rgb([seed, x as u8 * 6, y as u8 * 8])).save(&path).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn catalog(scratch: &Scratch) -> Catalog {
    Catalog::open(&scratch.path("data")).unwrap()
}

fn loose(library_id: i64) -> Place {
    Place { library_id, roll_id: None }
}

#[test]
fn starts_empty_and_keeps_what_is_made_across_opens() {
    let scratch = Scratch::new();
    {
        let c = catalog(&scratch);
        assert!(c.snapshot().unwrap().libraries.is_empty());
        let personal = c.create_library("Personal").unwrap();
        c.create_roll(personal.id, "Portra 400 #1").unwrap();
        c.create_library("Client work").unwrap();
    }
    let c = catalog(&scratch);
    let snapshot = c.snapshot().unwrap();
    let names: Vec<_> = snapshot.libraries.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Personal", "Client work"]);
    assert_eq!(snapshot.rolls.len(), 1);
    assert_eq!(snapshot.rolls[0].library_id, snapshot.libraries[0].id);
}

#[test]
fn imports_in_place_and_numbers_frames_on_a_roll() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Personal").unwrap();
    let roll = c.create_roll(library.id, "HP5").unwrap();
    let a = scratch.image("scans/a.png", 1);
    let b = scratch.image("scans/b.png", 2);
    let c3 = scratch.image("scans/c.png", 3);

    let loose_images = c.import(&[a.clone()], loose(library.id), false).unwrap();
    assert_eq!(loose_images.len(), 1);
    assert_eq!(loose_images[0].frame, None);
    assert_eq!(Path::new(&loose_images[0].path), std::fs::canonicalize(&a).unwrap());
    assert!(!loose_images[0].in_photo_folder);

    let frames = c
        .import(&[b, c3], Place { library_id: library.id, roll_id: Some(roll.id) }, false)
        .unwrap();
    assert_eq!(frames.iter().map(|i| i.frame).collect::<Vec<_>>(), [Some(1), Some(2)]);
    assert!(frames.iter().all(|i| i.roll_id == Some(roll.id) && i.library_id == library.id));
}

#[test]
fn a_file_already_in_the_catalog_is_not_imported_twice() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Personal").unwrap();
    let a = scratch.image("a.png", 1);
    assert_eq!(c.import(&[a.clone()], loose(library.id), false).unwrap().len(), 1);
    assert!(c.import(&[a.clone()], loose(library.id), false).unwrap().is_empty());
    assert!(c.find_by_file(&a).unwrap().is_some());
}

#[test]
fn copies_into_the_photo_folder_by_library_and_roll() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Client: work").unwrap();
    let roll = c.create_roll(library.id, "Roll 1").unwrap();
    let src = scratch.image("card/DSC_0001.png", 9);

    assert!(c.import(&[src.clone()], loose(library.id), true).is_err(), "no photo folder chosen yet");

    let photos = scratch.path("Photos");
    c.set_photo_folder(&photos).unwrap();
    let place = Place { library_id: library.id, roll_id: Some(roll.id) };
    let copied = c.import(&[src.clone()], place, true).unwrap();
    let expected = photos.join("Client_ work").join("Roll 1").join("DSC_0001.png");
    assert_eq!(PathBuf::from(&copied[0].path), expected);
    assert!(copied[0].in_photo_folder);
    assert!(expected.is_file() && src.is_file());

    // Recorded relative to the folder, so it follows the folder when it moves.
    let moved = scratch.path("Moved photos");
    std::fs::rename(&photos, &moved).unwrap();
    c.set_photo_folder(&moved).unwrap();
    let image = c.image(copied[0].id).unwrap();
    assert!(!image.missing);
    assert!(image.path.starts_with(&*moved.to_string_lossy()));
}

#[test]
fn edits_are_stored_and_read_back() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Personal").unwrap();
    let image = c.import(&[scratch.image("a.png", 1)], loose(library.id), false).unwrap().remove(0);
    assert_eq!(c.edit(image.id).unwrap(), None);
    assert_eq!(image.edited, None);

    let saved = c.save_edit(image.id, r#"{"version":1}"#).unwrap();
    assert_eq!(c.edit(image.id).unwrap().as_deref(), Some(r#"{"version":1}"#));
    assert_eq!(c.image(image.id).unwrap().edited, Some(saved));
    assert!(c.save_edit(9999, "{}").is_err());
}

#[test]
fn moving_onto_a_roll_numbers_on_and_a_roll_keeps_its_library() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let first = c.create_library("First").unwrap();
    let second = c.create_library("Second").unwrap();
    let roll = c.create_roll(second.id, "Tri-X").unwrap();
    let images = c
        .import(&[scratch.image("a.png", 1), scratch.image("b.png", 2)], loose(first.id), false)
        .unwrap();
    let ids: Vec<i64> = images.iter().map(|i| i.id).collect();

    // The library given is overruled by the roll's own.
    c.move_images(&ids, Place { library_id: first.id, roll_id: Some(roll.id) }).unwrap();
    for (n, &id) in ids.iter().enumerate() {
        let image = c.image(id).unwrap();
        assert_eq!((image.library_id, image.roll_id, image.frame), (second.id, Some(roll.id), Some(n as i64 + 1)));
    }

    c.move_images(&ids[..1], loose(first.id)).unwrap();
    let image = c.image(ids[0]).unwrap();
    assert_eq!((image.library_id, image.roll_id, image.frame), (first.id, None, None));
}

#[test]
fn deleting_a_roll_leaves_its_frames_loose_and_deleting_a_library_takes_everything() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Personal").unwrap();
    let roll = c.create_roll(library.id, "Ektar").unwrap();
    let file = scratch.image("a.png", 1);
    let image = c.import(&[file.clone()], Place { library_id: library.id, roll_id: Some(roll.id) }, false).unwrap();

    c.update_roll(roll.id, "Ektar 100", &json!({ "filmStock": "Kodak Ektar 100", "boxIso": "100" })).unwrap();
    let rolls = c.rolls().unwrap();
    assert_eq!(rolls[0].name, "Ektar 100");
    assert_eq!(rolls[0].info["filmStock"], "Kodak Ektar 100");

    c.delete_roll(roll.id).unwrap();
    let image = c.image(image[0].id).unwrap();
    assert_eq!((image.library_id, image.roll_id, image.frame), (library.id, None, None));

    std::fs::write(c.thumbnail_path(image.id), b"jpeg").unwrap();
    c.delete_library(library.id).unwrap();
    assert!(c.image(image.id).is_err());
    assert!(!c.thumbnail_path(image.id).exists());
    assert!(file.is_file(), "the file itself stays");
}

#[test]
fn a_missing_file_is_found_again_only_if_it_is_the_same_file() {
    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let library = c.create_library("Personal").unwrap();
    let file = scratch.image("old/a.png", 1);
    let other = scratch.image("other.png", 2);
    let image = c.import(&[file.clone()], loose(library.id), false).unwrap().remove(0);

    let moved = scratch.path("new/a.png");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&file, &moved).unwrap();
    assert!(c.image(image.id).unwrap().missing);

    assert!(c.relink(image.id, &other).is_err());
    let found = c.relink(image.id, &moved).unwrap();
    assert!(!found.missing);
    assert_eq!(PathBuf::from(found.path), std::fs::canonicalize(&moved).unwrap());
}

#[test]
fn fingerprints_tell_files_apart_and_folders_list_only_images() {
    let scratch = Scratch::new();
    let a = scratch.image("roll/02.png", 1);
    let b = scratch.image("roll/01.png", 2);
    std::fs::write(scratch.path("roll/notes.txt"), "scanned at 3200dpi").unwrap();
    assert_ne!(fingerprint(&a).unwrap(), fingerprint(&b).unwrap());
    assert_eq!(fingerprint(&a).unwrap(), fingerprint(&a).unwrap());

    let listed = folder_images(&scratch.path("roll"), &["png".to_string()]).unwrap();
    assert_eq!(listed, [b, a]);
}

#[test]
fn a_thumbnail_source_is_shrunk_to_the_thumbnail_edge() {
    let scratch = Scratch::new();
    let path = scratch.path("big.png");
    ImageBuffer::from_fn(1600, 900, |x, _| Rgb([(x % 256) as u8, 0, 0])).save(&path).unwrap();
    let source = ThumbnailSource::decode(&path).unwrap();
    assert_eq!((source.width, source.height), (768, 432));
    assert_eq!(source.pixels.len(), 768 * 432 * source.channels);
}

#[test]
fn a_thumbnail_renders_the_crop_as_a_jpeg() {
    use open_darkroom_lib::crop::CropPayload;
    use open_darkroom_lib::export_rendering::SlidersPayload;
    use open_darkroom_lib::thumbnails::render_thumbnail;

    let scratch = Scratch::new();
    let path = scratch.path("wide.png");
    ImageBuffer::from_fn(1200, 800, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128])).save(&path).unwrap();
    let source = ThumbnailSource::decode(&path).unwrap();
    let crop = CropPayload { w: 0.5, ..CropPayload::FULL_FRAME };

    let jpeg = pollster::block_on(render_thumbnail(&source, &SlidersPayload::default(), &crop, &[])).unwrap();
    let decoded = image::load_from_memory(&jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (384, 512));
}
