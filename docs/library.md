# The library

How the library is meant to behave, written down before it is built. The first
version is a prototype: anything marked **open** is to be settled by trying it,
not decided here.

## What it is for

Today the app opens one image at a time and forgets it when the next one opens.
The library keeps every image you have worked on, with its edit, and lets you
group them the way film is actually shot: in rolls. Opening the app gives you
all of it at once, and none of it is stuck on the computer it was made on.

Backing out of the editor takes you to the library. The side bars slide off to
the window edges and the image flies back into its place on a grid. Opening an
image from the grid plays the same motion in reverse.

## Concepts

| Thing | What it holds |
|---|---|
| **Catalog** | The record of everything opened on this computer: every library, roll and image, with their edits and thumbnails. One per install, and it can be exported whole. |
| **Library** | A named group of images and rolls, such as "Personal" or "Client work". |
| **Roll** | Inside a library: a group of images like a library, with film information on top (how it was shot and developed) and its frames in order. |
| **Image** | One file on disk: where it is, its edit, how it was shot, and its thumbnail. |

```
Catalog
├── Library
│   ├── Image
│   └── Roll
│       └── Image (frame)
└── Library
    └── …
```

- Opening the app shows the whole catalog.
- Libraries sit at the top of the catalog and never inside one another.
- A roll holds images the way a library does, and always sits inside a library.
  Rolls do not nest either.
- An image is either loose in a library or a frame on a roll.

### Roll information

Roll level, all optional and free to fill in later:

- film stock, format, box ISO and the ISO it was rated at
- the dates it was shot over, and the date it was developed
- camera and any number of lenses
- home or lab development: the chemistry, dilution, time and temperature at
  home, or the lab and its chemistry; push or pull either way
- scanner, notes

Film stocks, cameras, lenses, scanners and home lab chemistry are kept in the
catalog and managed in Settings. A roll picks from them, or takes what is typed
as a new one; deleting one leaves its name on the rolls that used it.

Frame level: frame number, aperture and shutter if logged, location, notes.

The inversion is **not** shared across a roll. Every frame keeps its own black,
white and gamma points, like any other edit.

## Storage

**The catalog** is a database, likely SQLite, in the app's data directory next to
`settings.json` and `presets.json`. It holds libraries, rolls, image records and
edits, with thumbnails cached beside it.

It belongs to this computer, but nothing in it is tied to it:

- the whole catalog exports and imports as one bundle (see Exporting)
- any library or roll exports on its own and merges into another catalog; an
  imported roll goes into a library chosen on import
- **Open**: whether the catalog's location can also be moved, for example to an
  external drive. If it can, it is locked while open, so two computers sharing
  it through a synced folder cannot both write to it.

### Where the image files are

Importing offers both:

- **Copy into the photo folder.** A folder the catalog looks after, chosen in
  settings (such as `Pictures/Open Darkroom`). Its files go with the catalog
  when it is exported.
- **Reference in place.** The files stay where they are, such as on an archive
  drive, and the catalog only points at them.

Files in the photo folder are recorded relative to it, so they survive the
folder moving. Files outside it are recorded by path and by a fingerprint (their
size plus a hash of their first bytes). A file that has moved, or a catalog
imported on another computer, finds it again from the fingerprint, or the user
is asked to point to it. Nothing is lost because a file went missing.
**Open**: which of the two is the default.

## Edits

The edit stored for each image is the `ImageEdit` document in
`src/lib/types/imageEdit.ts`. It is already versioned and read leniently, so an
edit written by one build still opens in another.

- Opening an image from the library restores its edit.
- Changes are saved as they are made, so there is no save step and nothing is
  lost on a crash.
- `editFitsSource` guards against a file that changed size underneath its edit.
  Heal and clone strokes are placed in full-resolution pixels.
- Undo history belongs to the session and is not stored. **Open**: whether it
  should survive leaving an image.

## The editor and the library side by side

The editor's state lives in the editor store (`src/lib/editor/editor.svelte.ts`)
rather than in a component. While the library is up, the editor is hidden, not
unmounted:

- its store, renderer and GPU textures stay as they were
- going back to the same image is instant
- WebGPU is never set up twice

Opening a different image replaces what the editor holds, as it does today.

### The fly-back transition

Done by hand with the FLIP technique (record first and last positions, then
animate between them), not the View Transitions API:

- View Transitions only exist on recent versions of macOS's WebKit, so the two
  platforms would behave differently.
- The preview is a WebGPU canvas that may be zoomed into part of the image. A
  View Transition would snapshot exactly that.

On leaving the editor, the view fits to the crop and the image's thumbnail takes
the canvas's place. The thumbnail then animates from the editor's frame to its
grid cell while the side bars slide away. Entering runs the same steps backwards
and hands over to the live canvas once it has drawn.

## Exporting

| Export | What it is for | What it contains |
|---|---|---|
| **The catalog** | Moving to a new computer, backing up | Every library, roll and edit, with thumbnails, and the originals if wanted. Importing it on another computer restores the lot |
| **A library or roll as a bundle** | Sharing, archiving, merging into another catalog | Originals, edits, roll and frame information and thumbnails in one file (such as `.odroll`) that another catalog imports |
| **A library or roll, edits only** | The other person already has the scans | Edits and roll information, matched to their files by fingerprint on import, so names and folders need not agree |
| **Images**: one, a selection, a roll or a library | Delivering finished photos | Rendered files from the existing export, in a batch |
| **One image's edit** | Passing a single look around | A small file next to the image, the way presets export as `.odpreset` |

## Backend

These land together with the library:

- **Open by path.** `open_image_file` currently shows the file picker itself. The
  library needs to open a known path, with the picker kept for importing.
- **Thumbnails.** Rendered by the backend through the same WGSL as export
  (`export_rendering.rs`), with each image's edit applied. The grid then shows
  edited images without opening them. Kept with the catalog and redone when an
  edit changes.
- **One image in memory.** The backend holds exactly one full-resolution image
  (`ImageState`). Making thumbnails must not push out the open image, or hold
  two full-resolution buffers at once.
- **Metadata.** `metadata::read(path)` already reads how a file was shot without
  decoding its pixels. The library uses it on import, for display and to prefill
  frame information.

## Already in place

- The editor moved into its own store and components, so it can be hidden and
  come back unchanged (alayshoo/Open-Darkroom#62).
- Camera metadata is read on open and shown in the info panel
  (alayshoo/Open-Darkroom#63).
- The first prototype: the catalog (`catalog.rs`, SQLite), opening by path,
  thumbnails rendered through the export WGSL, edits saved as they are made,
  the library view with roll and frame information, and the fly-back. Not yet:
  exporting, albums, finding a moved file by fingerprint without being asked.

### What the prototype settled on

To be revisited once it has been used:

- Imports are referenced in place by default. Copying into the photo folder is
  a setting, and puts each library and roll in a folder of its own.
- An image has one home, loose in a library or on one roll. There is no inbox:
  opening a file adds it to the library or roll being looked at, making a first
  library if there is none.
- Rolls are made both ways: a folder imported as a roll, or a new roll that
  frames are added or moved to.
- Undo history stays with the session.
- A roll and a library are renamed by double-clicking their name.

## Open questions

Settled by the prototype, not ahead of it:

- **Where an image lives.** The proposal: each image has one home, either loose
  in a library or on one roll. That keeps one edit, one roll and one thumbnail
  per file with no ambiguity. Rolls are not tied to the folders the files sit in,
  since scanner software often puts a whole session in one folder and folders get
  renamed. An image in several places at once is better served later by albums.
  **Open**: whether images can also sit loose at the top of the catalog, as an
  inbox not yet sorted into a library.
- **How rolls are made.** Importing a folder as a roll, making an empty roll and
  dragging frames in, or both.
- **Undo history.** Kept per session only, or stored with the edit.

## Later

Out of scope for the first version, and possible without changing the model
above:

- **Albums**: groupings that cut across libraries and rolls, picked by hand or
  kept up to date by a rule.
- **Virtual copies**: one file with more than one edit.
- **Contact sheets**: a roll laid out as one, on screen and as an export.
- **Roll information in exports**: film stock, camera and lens written into the
  exported file's metadata.
- **Shooting log**: a roll's information as a spreadsheet.
- **Sidecar files**: an optional copy of each edit next to its image.
