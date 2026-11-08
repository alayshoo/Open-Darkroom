// src-tauri/tests/catalog.rs
//
// The catalog against real files in a scratch directory: libraries, rolls,
// importing by reference and by copy, edits, moving and finding files again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use image::{ImageBuffer, Rgb};
use serde_json::json;

use open_darkroom_lib::catalog::{fingerprint, folder_images, Catalog, GearDetails, GearFields, Place, RollGear};
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

    c.update_roll(roll.id, "Ektar 100", &json!({ "filmStock": "Kodak Ektar 100", "boxIso": "100" }), None, &RollGear::default()).unwrap();
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

#[test]
fn a_new_catalog_is_seeded_with_stocks_once() {
    let scratch = Scratch::new();
    let seeded = open_darkroom_lib::catalog::film_stocks_seeds_len();
    let stocks = catalog(&scratch).film_stocks().unwrap();
    assert_eq!(stocks.len(), seeded);
    let portra = stocks.iter().find(|s| s.fields.label() == "Kodak Portra 400").unwrap();
    assert_eq!((portra.fields.box_iso, portra.fields.process.as_str()), (Some(400), "C-41"));
    assert_eq!(
        [&portra.fields.colour, &portra.fields.colour2, &portra.fields.colour3].map(|c| c.as_deref()),
        [Some("#ecb445"), Some("#7174b5"), Some("#793a8e")],
    );

    // A deleted stock stays deleted the next time the catalog opens.
    catalog(&scratch).delete_film_stock(portra.id).unwrap();
    assert_eq!(catalog(&scratch).film_stocks().unwrap().len(), seeded - 1);
}

#[test]
fn a_catalog_seeded_before_a_new_batch_gets_it_once() {
    let scratch = Scratch::new();
    let seeded = open_darkroom_lib::catalog::film_stocks_seeds_len();
    let dir = scratch.path("data");
    {
        // As a catalog from before the second batch: only the first, with one of it deleted.
        let c = Catalog::open(&dir).unwrap();
        drop(c);
        let conn = rusqlite::Connection::open(dir.join("catalog.sqlite")).unwrap();
        conn.execute_batch(
            "DELETE FROM film_stocks WHERE maker IN ('Ilford', 'Kentmere', 'Harman', 'Lomography');
             DELETE FROM film_stocks WHERE name = 'Gold 200';
             UPDATE meta SET value = '1' WHERE key = 'stocks_seeded';",
        )
        .unwrap();
    }
    let stocks = Catalog::open(&dir).unwrap().film_stocks().unwrap();
    assert_eq!(stocks.len(), seeded - 1);
    assert!(!stocks.iter().any(|s| s.fields.name == "Gold 200"));
    let hp5 = stocks.iter().find(|s| s.fields.label() == "Ilford HP5 Plus 400").unwrap();
    assert_eq!((hp5.fields.box_iso, hp5.fields.colour.as_deref(), hp5.fields.colour2.as_deref()), (Some(400), Some("#ffffff"), Some("#49a057")));
    let metropolis = stocks.iter().find(|s| s.fields.name == "LomoChrome Metropolis 100-400").unwrap();
    assert_eq!(metropolis.fields.box_iso, None);

    assert_eq!(Catalog::open(&dir).unwrap().film_stocks().unwrap().len(), seeded - 1);
}

#[test]
fn stocks_are_added_and_edited_and_a_roll_keeps_its_stock_name_when_it_goes() {
    use open_darkroom_lib::catalog::StockFields;

    let scratch = Scratch::new();
    let c = catalog(&scratch);
    let mut fields = StockFields {
        maker: "Foma".into(),
        name: "Fomapan 200".into(),
        box_iso: Some(200),
        kind: "blackAndWhite".into(),
        process: "B&W".into(),
        formats: vec!["35mm".into(), "120".into()],
        colour: Some("#3a6ea5".into()),
        colour2: None,
        colour3: None,
    };
    let stock = c.create_film_stock(&fields).unwrap();
    fields.colour2 = Some("#1c1c1c".into());
    fields.colour3 = Some("#642f88".into());
    c.update_film_stock(stock.id, &fields).unwrap();
    let stored = c.film_stocks().unwrap().into_iter().find(|s| s.id == stock.id).unwrap();
    assert_eq!(stored.fields, fields);

    let library = c.create_library("Personal").unwrap();
    let roll = c.create_roll(library.id, "Fog").unwrap();
    c.update_roll(roll.id, "Fog", &json!({ "boxIso": "200" }), Some(stock.id), &RollGear::default()).unwrap();
    assert_eq!(c.rolls().unwrap()[0].stock_id, Some(stock.id));

    c.delete_film_stock(stock.id).unwrap();
    let roll = &c.rolls().unwrap()[0];
    assert_eq!(roll.stock_id, None);
    assert_eq!(roll.info["filmStock"], "Foma Fomapan 200");
    assert_eq!(roll.info["boxIso"], "200");
}

#[test]
fn a_catalog_from_before_film_stocks_is_brought_up_to_date() {
    let scratch = Scratch::new();
    let dir = scratch.path("data");
    std::fs::create_dir_all(&dir).unwrap();
    {
        let old = rusqlite::Connection::open(dir.join("catalog.sqlite")).unwrap();
        old.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('schema', '1');
             CREATE TABLE libraries (id INTEGER PRIMARY KEY, name TEXT NOT NULL, created INTEGER NOT NULL);
             CREATE TABLE rolls (id INTEGER PRIMARY KEY, library_id INTEGER NOT NULL, name TEXT NOT NULL,
                                 info TEXT NOT NULL DEFAULT '{}', created INTEGER NOT NULL);
             INSERT INTO libraries VALUES (1, 'Personal', 1);
             INSERT INTO rolls VALUES (1, 1, 'Old roll', '{\"filmStock\":\"HP5\"}', 2);",
        )
        .unwrap();
    }
    let c = Catalog::open(&dir).unwrap();
    let rolls = c.rolls().unwrap();
    assert_eq!((rolls[0].name.as_str(), rolls[0].stock_id), ("Old roll", None));
    assert_eq!(rolls[0].info["filmStock"], "HP5");
    assert!(!c.film_stocks().unwrap().is_empty());
}

fn gear(kind: &str, maker: &str, name: &str, mount: &str) -> GearFields {
    GearFields {
        kind: kind.into(),
        maker: maker.into(),
        name: name.into(),
        details: GearDetails { mount: mount.into(), ..Default::default() },
    }
}

#[test]
fn gear_is_kept_with_its_details_and_rolls_point_at_it() {
    let scratch = Scratch::new();
    let c = Catalog::open(&scratch.path("data")).unwrap();
    let camera = c.create_gear(&gear("camera", "Nikon", "FM2", "F")).unwrap();
    let fifty = c.create_gear(&gear("lens", "Nikon", "Nikkor 50mm f/1.8 AI-s", "F")).unwrap();
    let wide = c.create_gear(&gear("lens", "Nikon", "Nikkor 28mm f/2.8 AI-s", "F")).unwrap();
    let mut rodinal = gear("chemistry", "Adox", "Rodinal", "");
    rodinal.details.process = "B&W".into();
    let rodinal = c.create_gear(&rodinal).unwrap();

    let mut renamed = camera.fields.clone();
    renamed.name = "FM2n".into();
    c.update_gear(camera.id, &renamed).unwrap();
    let stored = c.gear().unwrap();
    assert_eq!(stored.len(), 4);
    assert_eq!(stored.iter().find(|g| g.id == camera.id).unwrap().fields, renamed);
    assert_eq!(stored.iter().find(|g| g.id == rodinal.id).unwrap().fields.details.process, "B&W");

    let library = c.create_library("Personal").unwrap();
    let roll = c.create_roll(library.id, "Lisbon").unwrap();
    let links = RollGear { camera: Some(camera.id), lenses: vec![fifty.id, wide.id], scanner: None, chemistry: Some(rodinal.id) };
    c.update_roll(roll.id, "Lisbon", &json!({ "lenses": "Old zoom" }), None, &links).unwrap();
    assert_eq!(c.rolls().unwrap()[0].gear, links);
    assert_eq!(c.snapshot().unwrap().gear.len(), 4);
}

#[test]
fn deleting_gear_leaves_its_name_on_the_rolls_that_used_it() {
    let scratch = Scratch::new();
    let c = Catalog::open(&scratch.path("data")).unwrap();
    let camera = c.create_gear(&gear("camera", "Mamiya", "7", "")).unwrap();
    let lens = c.create_gear(&gear("lens", "Mamiya", "80mm f/4", "Mamiya 7")).unwrap();
    let other = c.create_gear(&gear("lens", "Mamiya", "43mm f/4.5", "Mamiya 7")).unwrap();
    let library = c.create_library("Personal").unwrap();
    let roll = c.create_roll(library.id, "Night walks").unwrap();
    let links = RollGear { camera: Some(camera.id), lenses: vec![lens.id, other.id], ..Default::default() };
    c.update_roll(roll.id, "Night walks", &json!({ "lenses": "Old zoom" }), None, &links).unwrap();

    c.delete_gear(camera.id).unwrap();
    c.delete_gear(lens.id).unwrap();
    let roll = &c.rolls().unwrap()[0];
    assert_eq!(roll.gear, RollGear { lenses: vec![other.id], ..Default::default() });
    assert_eq!(roll.info["camera"], "Mamiya 7");
    assert_eq!(roll.info["lenses"], "Old zoom, Mamiya 80mm f/4");
    assert_eq!(c.gear().unwrap().len(), 1);
}
