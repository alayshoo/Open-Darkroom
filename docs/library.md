# The library

How the library is meant to behave, written down before it is built. The first
version is a prototype: anything marked **open** is to be settled by trying it,
not decided here.

## What it is for

Today the app opens one image at a time and forgets it when the next one opens.
The library keeps every image you have worked on, with its edit, and lets you
group them the way film is actually shot: in rolls.

Backing out of the editor takes you to the library. The side bars slide off to
the window edges and the image flies back into its place on a grid. Opening an
image from the grid plays the same motion in reverse.

## Concepts

| Thing | What it holds |
|---|---|
| **Catalog** | Everything below, in one database. There is one catalog per user. |
| **Library** | A named space of images and rolls, such as "Personal" or "Client work". |
| **Roll** | A film roll: how it was shot and developed, and the frames on it, in order. |
| **Image** | One file on disk: where it is, its edit, how it was shot, and its thumbnail. |

A library holds images directly, rolls, or both. An image is either loose in a
library or a frame on a roll.

### Roll information

Roll level, all optional and free to fill in later:

- film stock, box ISO and the ISO it was rated at, push or pull
- camera and lens(es)
- developer, dilution, time and temperature, home or lab development
- date shot, date developed
- scanner, notes

Frame level: frame number, aperture and shutter if logged, location, notes.

The inversion is **not** shared across a roll. Every frame keeps its own black,
white and gamma points, like any other edit.

## Storage

**A central catalog** in the app's data directory, next to `settings.json` and
`presets.json`. SQLite is the likely choice. It holds libraries, rolls, image
records and edits. Thumbnails are cached as files beside it.

Image files stay where they are. The catalog records each file's path and a
fingerprint (its size plus a hash of its first bytes). A file that has moved can
be found again from the fingerprint, or the user is asked to point to it.
Nothing in the library is lost because a file went missing.

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

## Backend

These land together with the library:

- **Open by path.** `open_image_file` currently shows the file picker itself. The
  library needs to open a known path, with the picker kept for importing.
- **Thumbnails.** Rendered by the backend through the same WGSL as export
  (`export_rendering.rs`), with each image's edit applied. The grid then shows
  edited images without opening them. Cached on disk and redone when an edit
  changes.
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

## Open questions

Settled by the prototype, not ahead of it:

- **What a library is.** The proposal: libraries are virtual and each image has
  one home, either loose in one library or on one roll. That keeps one edit, one
  roll and one thumbnail per file with no ambiguity. Libraries are not tied to
  folders on disk, since scanner software often puts a whole session in one
  folder and folders get renamed. The alternative, an image in several places at
  once, is better served later by albums.
- **How rolls are made.** Importing a folder as a roll, making an empty roll and
  dragging frames in, or both.
- **Importing.** Reference files in place (proposed), or offer to copy them into
  a managed folder as well.
- **Undo history.** Kept per session only, or stored with the edit.

## Later

Out of scope for the first version, and possible without changing the model
above:

- **Albums**: groupings that cut across libraries, picked by hand or kept up to date by a rule.
- **Virtual copies**: one file with more than one edit.
- **Contact sheets**: a roll laid out as one, on screen and as an export.
- **Roll information in exports**: film stock, camera and lens written into the
  exported file's metadata.
- **Sidecar files**: an optional copy of each edit next to its image.
