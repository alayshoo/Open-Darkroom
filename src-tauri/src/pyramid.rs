// src-tauri/src/pyramid.rs
//
// The multi-resolution pyramid the preview streams its tiles from. Level 0 is
// the full-resolution image; each level above it halves both dimensions, until
// the longest edge fits within PREVIEW_MAX_EDGE.

use half::f16;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use serde::Serialize;
use tauri::ipc::Response;

use crate::color::{
    linear_u16_to_f16_bytes, linearize_u16, to_linear_u16, LINEAR_BYTES_PER_PIXEL, LUT_SIZE,
};
use crate::icc::SourceSpace;
use crate::image_opening::{resize_to, ImageState, PREVIEW_MAX_EDGE};

/// Edge length of a full tile. Tiles on the right and bottom edges of a level
/// are cut short by however much of the level is left.
pub const TILE_SIZE: u32 = 512;

/// Bytes preceding each tile's pixels in a batch: its column, row, width and
/// height, u32 LE.
pub const TILE_HEADER_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LevelSize {
    pub width: u32,
    pub height: u32,
}

impl LevelSize {
    pub fn columns(&self) -> u32 {
        self.width.div_ceil(TILE_SIZE)
    }

    pub fn rows(&self) -> u32 {
        self.height.div_ceil(TILE_SIZE)
    }
}

/// The size of every level, finest first. An image already within
/// `PREVIEW_MAX_EDGE` is a pyramid of one level.
pub fn level_sizes(width: u32, height: u32) -> Vec<LevelSize> {
    let mut sizes = vec![LevelSize { width, height }];
    loop {
        let last = sizes[sizes.len() - 1];
        if last.width.max(last.height) <= PREVIEW_MAX_EDGE {
            return sizes;
        }
        sizes.push(LevelSize {
            width: last.width.div_ceil(2),
            height: last.height.div_ceil(2),
        });
    }
}

/// One tile's linear RGBA f16 LE pixels, `width` × `height` of them.
pub struct Tile {
    pub column: u32,
    pub row: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The tile size and the size of every level, finest first, as the frontend
/// receives it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PyramidStructure {
    pub tile_size: u32,
    pub levels: Vec<LevelSize>,
}

pub struct Pyramid {
    source: Arc<Vec<u16>>,
    channels: usize,
    space: SourceSpace,
    sizes: Vec<LevelSize>,
    /// Built on first use and kept for the life of the image, as linear u16.
    /// Level 0 is read straight from the source instead, so its slot stays empty.
    levels: Vec<OnceLock<Result<Vec<u16>, String>>>,
    /// Builds run one at a time: each holds a linearised copy of the whole
    /// source while it runs, and two at once would double that peak.
    build_lock: Mutex<()>,
    f16_luts: [Vec<f16>; 3],
    u16_luts: [Vec<u16>; 3],
}

impl Pyramid {
    pub fn new(
        source: Arc<Vec<u16>>,
        width: u32,
        height: u32,
        channels: usize,
        space: SourceSpace,
    ) -> Self {
        let sizes = level_sizes(width, height);
        Self {
            source,
            channels,
            f16_luts: space.luts(f16::from_f32),
            u16_luts: space.luts(|v| (v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32).round() as u16),
            space,
            levels: sizes.iter().map(|_| OnceLock::new()).collect(),
            sizes,
            build_lock: Mutex::new(()),
        }
    }

    pub fn structure(&self) -> PyramidStructure {
        PyramidStructure {
            tile_size: TILE_SIZE,
            levels: self.sizes.clone(),
        }
    }

    pub fn sizes(&self) -> &[LevelSize] {
        &self.sizes
    }

    /// Whether a reduced level is built and ready to cut tiles from. Level 0
    /// always is.
    pub fn is_ready(&self, level: usize) -> bool {
        level == 0 || self.levels.get(level).is_some_and(|l| l.get().is_some())
    }

    /// One tile of a level, building the level first if it is not yet. Not to
    /// be called from inside the rayon pool: a build blocks on the level's lock
    /// while running parallel work of its own.
    pub fn tile(&self, level: usize, column: u32, row: u32) -> Result<Tile, String> {
        let size = *self
            .sizes
            .get(level)
            .ok_or_else(|| format!("No pyramid level {level}"))?;
        if column >= size.columns() || row >= size.rows() {
            return Err(format!("No tile {column},{row} on pyramid level {level}"));
        }

        let x = column * TILE_SIZE;
        let y = row * TILE_SIZE;
        let width = TILE_SIZE.min(size.width - x);
        let height = TILE_SIZE.min(size.height - y);

        let pixels = if level == 0 {
            let region = cut(&self.source, size.width, self.channels, x, y, width, height);
            let baked = self.space.bake(&region, self.channels)?;
            linearize_u16(
                baked.as_deref().unwrap_or(&region),
                self.channels,
                &self.f16_luts,
            )
        } else {
            let region = cut(
                self.level(level)?,
                size.width,
                self.channels,
                x,
                y,
                width,
                height,
            );
            linear_u16_to_f16_bytes(&region, self.channels)
        };

        Ok(Tile {
            column,
            row,
            width,
            height,
            pixels,
        })
    }

    /// Start building the levels either side of `level` in the background, so
    /// a zoom in or out finds them ready. Returns the builds it started.
    pub fn prefetch_around(self: &Arc<Self>, level: usize) -> Vec<JoinHandle<()>> {
        [level.checked_sub(1), level.checked_add(1)]
            .into_iter()
            .flatten()
            .filter(|&l| l < self.sizes.len() && !self.is_ready(l))
            .map(|l| {
                // Weak, so a build still queued when the image is closed
                // never starts.
                let pyramid = Arc::downgrade(self);
                std::thread::spawn(move || {
                    if let Some(pyramid) = pyramid.upgrade() {
                        let _ = pyramid.level(l);
                    }
                })
            })
            .collect()
    }

    /// A reduced level, building it if it is not yet. Each is resampled from
    /// the full-resolution source rather than from its neighbour, so any level
    /// can be built without the ones between it and the source.
    fn level(&self, level: usize) -> Result<&[u16], String> {
        debug_assert!(level > 0, "level 0 is read from the source");
        self.levels[level]
            .get_or_init(|| {
                let _building = self.build_lock.lock().unwrap_or_else(|e| e.into_inner());
                let baked = self.space.bake(&self.source, self.channels)?;
                let src = baked.as_deref().unwrap_or(&self.source);
                let linear = to_linear_u16(src, self.channels, &self.u16_luts);
                let (full, size) = (self.sizes[0], self.sizes[level]);
                Ok(resize_to(
                    &linear,
                    full.width,
                    full.height,
                    self.channels,
                    size.width,
                    size.height,
                ))
            })
            .as_deref()
            .map_err(Clone::clone)
    }
}

/// The `width` × `height` region at `x`, `y` of an image `stride` pixels wide.
fn cut(
    src: &[u16],
    stride: u32,
    channels: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Vec<u16> {
    let row_len = width as usize * channels;
    let mut out = Vec::with_capacity(row_len * height as usize);
    for row in y..y + height {
        let at = (row as usize * stride as usize + x as usize) * channels;
        out.extend_from_slice(&src[at..at + row_len]);
    }
    out
}

/// Serialise a batch of tiles into the layout the frontend parses:
///
/// | size        | contents                              |
/// |-------------|---------------------------------------|
/// | 4           | tile count, u32 LE                    |
/// | 16          | column, row, width, height, u32 LE    |
/// | w × h × 8   | RGBA f16 LE pixels                    |
/// | …           | header and pixels again, per tile     |
pub fn build_tile_payload(tiles: &[Tile]) -> Vec<u8> {
    let len = 4 + tiles
        .iter()
        .map(|t| TILE_HEADER_BYTES + t.pixels.len())
        .sum::<usize>();
    let mut payload = Vec::with_capacity(len);
    payload.extend_from_slice(&(tiles.len() as u32).to_le_bytes());
    for tile in tiles {
        debug_assert_eq!(
            tile.pixels.len(),
            tile.width as usize * tile.height as usize * LINEAR_BYTES_PER_PIXEL
        );
        for v in [tile.column, tile.row, tile.width, tile.height] {
            payload.extend_from_slice(&v.to_le_bytes());
        }
        payload.extend_from_slice(&tile.pixels);
    }
    payload
}

// ── Tauri commands ────────────────────────────────────────────────────────────

fn open_pyramid(state: &ImageState) -> Result<Arc<Pyramid>, String> {
    let guard = state.lock().unwrap();
    let img = guard.as_ref().ok_or("No image loaded")?;
    Ok(Arc::clone(&img.pyramid))
}

#[tauri::command]
pub(crate) async fn pyramid_structure(
    state: tauri::State<'_, ImageState>,
) -> Result<PyramidStructure, String> {
    Ok(open_pyramid(&state)?.structure())
}

/// The requested tiles of one level, each given as `[column, row]`. Once they
/// are cut, the levels either side start building in the background.
#[tauri::command]
pub(crate) async fn pyramid_tiles(
    state: tauri::State<'_, ImageState>,
    level: usize,
    tiles: Vec<[u32; 2]>,
) -> Result<Response, String> {
    let pyramid = open_pyramid(&state)?;

    // Off the async runtime: a level built on demand takes long enough to
    // stall whatever else that worker is carrying.
    let payload = tauri::async_runtime::spawn_blocking(move || {
        let cut = tiles
            .iter()
            .map(|&[column, row]| pyramid.tile(level, column, row))
            .collect::<Result<Vec<_>, _>>()?;
        pyramid.prefetch_around(level);
        Ok::<_, String>(build_tile_payload(&cut))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(Response::new(payload))
}
