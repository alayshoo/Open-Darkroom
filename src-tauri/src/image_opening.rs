// src-tauri/src/image_opening.rs

use fast_image_resize as fir;
use fast_image_resize::images::{Image, ImageRef};
use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use half::f16;
use image::{ImageBuffer, ImageDecoder, Rgba};

use crate::color::{
    linear_u16_to_f16_bytes, linearize_u16, to_linear_u16, LINEAR_BYTES_PER_PIXEL, LINEAR_F32_BYTES_PER_PIXEL,
    LUT_SIZE,
};
use crate::crop::{crop_view, CropPayload, CropView};
use crate::export_rendering::request_device;
use crate::retouch::{
    apply_retouch, block_box, follow_strokes, Followed, LinearSource, RetouchPasses, RetouchPayload, RetouchedBlocks,
    StrokeJournal,
};
use crate::full_resolution::FullResolution;
use crate::icc::{Matrix3, SourceSpace};
use crate::metadata::ImageMetadata;

/// The four-channel decode shape, produced whenever the source declares alpha.
pub type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

/// Longest edge of the preview handed to the frontend.
pub const PREVIEW_MAX_EDGE: u32 = 2048;

/// Total pixels in the overview handed to the frontend.
///
/// The overview always covers the whole frame, whatever the preview shows, so
/// the live histogram describes the image rather than the viewport. The budget
/// is area rather than edge length because otherwise extreme aspect ratios would
/// create low pixel sample counts.
pub const OVERVIEW_TARGET_PIXELS: u32 = 512 * 512;

/// Longest edge the overview may reach once the area budget is met.
///
/// A safety valve for extreme aspect ratios rather than a second budget: it
/// keeps a stitched strip inside the texture limits and never binds on an
/// ordinary frame.
pub const OVERVIEW_MAX_EDGE: u32 = 4096;

/// Bins in each per-channel histogram.
pub const HIST_BINS: usize = 256;

/// Pixels one worker bins before its partial counts are merged. Splitting any
/// finer would cost more in carrying the 3 KB of counters around than it saves.
const HIST_BLOCK_PIXELS: usize = 1 << 16;

/// Bytes preceding the pixel data in the frontend payload: the six dimensions
/// as u32 LE, the source → working-space matrix as 9 f32 LE, then a flags word.
/// The full dimensions are what anchors a slider specified in image pixels to a
/// downscaled preview.
pub const PAYLOAD_HEADER_BYTES: usize = 64;

// ── State ─────────────────────────────────────────────────────────────────────

pub struct OriginalImage {
    /// Full-resolution interleaved u16 pixels. Behind an `Arc` so export can
    /// take a reference-counted handle instead of copying the whole buffer.
    pub pixels_u16: Arc<Vec<u16>>,
    pub width: u32,
    pub height: u32,
    /// Samples per pixel: 3 when the source declared no alpha channel, 4 when
    /// it did. Sources without alpha are stored three-wide, which is a quarter
    /// less memory held for as long as the image is open.
    pub channels: usize,
    /// What the file said its numbers mean. Held for the life of the image
    /// because export has to decode the same buffer the same way, at its own
    /// table width.
    pub space: SourceSpace,
    /// What the preview's full-resolution texture is streamed from.
    pub full_resolution: Arc<FullResolution>,
    /// The frontend's tag for this open, so a band asked for an image that has
    /// since been replaced is refused rather than cut from the new one.
    pub id: u32,
    /// How the file says it was shot.
    pub metadata: ImageMetadata,
}

pub type ImageState = Mutex<Option<OriginalImage>>;

// ── Derived image data ────────────────────────────────────────────────────────

/// Per-channel histograms of the full-resolution image, binned in sRGB code values.
#[derive(Clone)]
pub struct Histograms {
    pub r: [u32; HIST_BINS],
    pub g: [u32; HIST_BINS],
    pub b: [u32; HIST_BINS],
}

/// Everything derived from a decoded image that the frontend needs, with no
/// Tauri or filesystem involvement — this is the unit the tests exercise.
pub struct PreparedImage {
    /// Linearised preview pixels: RGBA f16 little-endian, 8 bytes per pixel.
    pub preview_pixels: Vec<u8>,
    pub preview_width: u32,
    pub preview_height: u32,
    pub overview_pixels: Vec<u8>,
    pub overview_width: u32,
    pub overview_height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub histograms: Histograms,
    /// Source RGB → linear Rec.2020, for the GPU to apply after the darkroom
    /// block. Identity when the conversion was baked in below.
    pub to_working: Matrix3,
    /// Set when the profile had to be applied on the CPU, so the darkroom block
    /// runs in the working space rather than the source's own primaries.
    pub baked: bool,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Downscale so the longest edge is at most `max_edge`, preserving aspect ratio,
/// and return the pixels with their new dimensions. Images already within the
/// limit are returned unchanged.
///
/// Resampling happens in u16 space so the preview keeps the source precision.
/// `channels` is 3 or 4; `max_edge` must be greater than zero.
pub fn downscale_to_max_edge(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    max_edge: u32,
) -> (Vec<u16>, u32, u32) {
    assert!(max_edge > 0, "max_edge must be non-zero");

    if width.max(height) <= max_edge {
        return (src.to_vec(), width, height);
    }

    resample(
        src,
        width,
        height,
        channels,
        max_edge as f32 / width.max(height) as f32,
    )
}

/// Downscale to at most `target_pixels` in total, preserving aspect ratio, and
/// return the pixels with their new dimensions. Images already within the
/// budget are returned unchanged.
///
/// Budgeting area keeps the sample count, and so the resample factor and the
/// cost of anything reading the result, independent of aspect ratio.
/// `max_edge` caps the longest side afterwards, which only binds on aspect
/// ratios past roughly 64:1. Both must be greater than zero.
pub fn downscale_to_target_pixels(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    target_pixels: u32,
    max_edge: u32,
) -> (Vec<u16>, u32, u32) {
    assert!(target_pixels > 0, "target_pixels must be non-zero");
    assert!(max_edge > 0, "max_edge must be non-zero");

    let pixels = width as u64 * height as u64;
    if pixels <= target_pixels as u64 && width.max(height) <= max_edge {
        return (src.to_vec(), width, height);
    }

    let area_scale = (target_pixels as f64 / pixels as f64).sqrt() as f32;
    let edge_scale = max_edge as f32 / width.max(height) as f32;

    resample(
        src,
        width,
        height,
        channels,
        area_scale.min(edge_scale).min(1.0),
    )
}

/// Resample by `scale`, rounding to whole pixels and never below one.
fn resample(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    scale: f32,
) -> (Vec<u16>, u32, u32) {
    assert!(
        channels == 3 || channels == 4,
        "channels must be 3 (RGB) or 4 (RGBA), got {channels}"
    );

    let new_w = ((width as f32 * scale).round() as u32).max(1);
    let new_h = ((height as f32 * scale).round() as u32).max(1);

    let pixel_type = if channels == 4 {
        fir::PixelType::U16x4
    } else {
        fir::PixelType::U16x3
    };

    let src_image = ImageRef::new(width, height, bytemuck::cast_slice(src), pixel_type).unwrap();

    let mut dst_image = Image::new(
        new_w.try_into().unwrap(),
        new_h.try_into().unwrap(),
        pixel_type,
    );

    let mut resizer = fir::Resizer::new();
    resizer
        .resize(
            &src_image,
            &mut dst_image,
            &fir::ResizeOptions::new()
                .resize_alg(fir::ResizeAlg::Convolution(fir::FilterType::Bilinear)),
        )
        .unwrap();

    let dst_bytes = dst_image.into_vec();
    let u16_vec: Vec<u16> = dst_bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();

    (u16_vec, new_w, new_h)
}

/// R/G/B histograms of the full-resolution image. `bins` maps each u16 code
/// value to its bin, as [`SourceSpace::histogram_bins`] builds it. Alpha, where
/// present, is not binned.
///
/// Each worker bins into its own set of counters and the partials are summed at
/// the end, so the hot loop needs no atomics.
pub fn compute_histograms(src: &[u16], channels: usize, bins: &[u8]) -> Histograms {
    src.par_chunks(HIST_BLOCK_PIXELS * channels)
        .map(|block| {
            let mut counts = empty_bins();
            block
                .chunks_exact(channels)
                .for_each(|px| bin_pixel(&mut counts, px, bins));
            counts
        })
        .reduce(empty_bins, sum_bins)
        .into()
}

type Bins = ([u32; HIST_BINS], [u32; HIST_BINS], [u32; HIST_BINS]);

fn empty_bins() -> Bins {
    ([0u32; HIST_BINS], [0u32; HIST_BINS], [0u32; HIST_BINS])
}

fn bin_pixel(counts: &mut Bins, px: &[u16], bins: &[u8]) {
    counts.0[bins[px[0] as usize] as usize] += 1;
    counts.1[bins[px[1] as usize] as usize] += 1;
    counts.2[bins[px[2] as usize] as usize] += 1;
}

fn sum_bins(mut acc: Bins, part: Bins) -> Bins {
    for i in 0..HIST_BINS {
        acc.0[i] += part.0[i];
        acc.1[i] += part.1[i];
        acc.2[i] += part.2[i];
    }
    acc
}

impl From<Bins> for Histograms {
    fn from((r, g, b): Bins) -> Self {
        Histograms { r, g, b }
    }
}

/// [`compute_histograms`] over only the pixels inside a crop of an image
/// `width` pixels wide.
pub fn compute_histograms_in(
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    view: &CropView,
    bins: &[u8],
) -> Histograms {
    let [x0, y0, x1, y1] = view.source_texels(0..view.width, 0..view.height);
    let x0 = x0.clamp(0, width as i64) as u32;
    let x1 = x1.clamp(x0 as i64, width as i64) as u32;
    let y0 = y0.clamp(0, height as i64) as u32;
    let y1 = y1.clamp(y0 as i64, height as i64) as u32;

    (y0..y1)
        .into_par_iter()
        .fold(empty_bins, |mut counts, y| {
            for x in (x0..x1).filter(|&x| view.contains(x, y)) {
                let at = (y as usize * width as usize + x as usize) * channels;
                bin_pixel(&mut counts, &src[at..at + channels], bins);
            }
            counts
        })
        .reduce(empty_bins, sum_bins)
        .into()
}

/// The source code whose linearised value is nearest `v`, on a curve that
/// never falls.
fn encode_linear(lut: &[f32], v: f32) -> u16 {
    let above = lut.partition_point(|&l| l < v).min(lut.len() - 1);
    let below = above.saturating_sub(1);
    let nearest = if (lut[above] - v).abs() < (v - lut[below]).abs() { above } else { below };
    nearest as u16
}

/// What binning an image's pixels within a crop takes, with the curves
/// retouched pixels are encoded back through.
struct Binning<'a> {
    src: &'a [u16],
    width: u32,
    height: u32,
    channels: usize,
    view: &'a CropView,
    bins: &'a [u8],
    luts: &'a [Vec<f32>; 3],
}

impl Binning<'_> {
    /// Adds block `key`'s pixels within the crop to `histograms`, or takes
    /// them out: as `held` has them, or untouched when it is None.
    fn count_block(&self, histograms: &mut Histograms, key: (u32, u32), held: Option<&[u8]>, add: bool) {
        let [x0, y0, w, h] = block_box(key, self.width, self.height);
        let px = LINEAR_F32_BYTES_PER_PIXEL;
        for y in y0..y0 + h {
            for x in (x0..x0 + w).filter(|&x| self.view.contains(x, y)) {
                let at = (y as usize * self.width as usize + x as usize) * self.channels;
                let texel = ((y - y0) as usize * w as usize + (x - x0) as usize) * px;
                for (c, counts) in [&mut histograms.r, &mut histograms.g, &mut histograms.b].into_iter().enumerate() {
                    let code = match held {
                        Some(data) => {
                            let v = f32::from_le_bytes(data[texel + c * 4..texel + c * 4 + 4].try_into().unwrap());
                            encode_linear(&self.luts[c], v)
                        }
                        None => self.src[at + c],
                    };
                    let bin = &mut counts[self.bins[code as usize] as usize];
                    if add {
                        *bin += 1;
                    } else {
                        *bin -= 1;
                    }
                }
            }
        }
    }

    /// Takes the untouched pixels of every block `source`'s strokes wrote out
    /// of `histograms`, and puts the retouched ones in.
    fn rebin_retouched(&self, histograms: &mut Histograms, source: &LinearSource) {
        for (key, data) in source.blocks() {
            self.count_block(histograms, key, None, false);
            self.count_block(histograms, key, Some(data), true);
        }
    }
}

/// [`compute_histograms_in`] with `strokes` laid over the image first. Only
/// the blocks the strokes wrote are binned again: their untouched pixels come
/// out of the counts and the retouched ones, encoded back through `luts`, go in.
#[allow(clippy::too_many_arguments)]
pub fn compute_retouched_histograms_in(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &[u16],
    width: u32,
    height: u32,
    channels: usize,
    view: &CropView,
    bins: &[u8],
    luts: &[Vec<f32>; 3],
    strokes: &[RetouchPayload],
) -> Result<Histograms, String> {
    let mut histograms = compute_histograms_in(src, width, height, channels, view, bins);
    if strokes.is_empty() {
        return Ok(histograms);
    }
    let mut source = LinearSource::new(src, width, height, channels, luts);
    apply_retouch(device, queue, &mut source, strokes)?;
    Binning { src, width, height, channels, view, bins, luts }.rebin_retouched(&mut histograms, &source);
    Ok(histograms)
}

/// Bytes of retouched blocks and journal kept between histogram requests. The
/// journal gives way first; once the blocks alone pass it nothing is kept.
pub const MAX_RETOUCHED_BYTES: usize = 512 << 20;

/// The retouched histograms of one image, kept between requests: the strokes
/// laid over it with what each wrote over, and the counts for the last crop.
pub struct RetouchedHistograms {
    journal: StrokeJournal,
    blocks: RetouchedBlocks,
    blocks_bytes: usize,
    crop: Option<CropPayload>,
    histograms: Option<Histograms>,
    budget: usize,
}

impl RetouchedHistograms {
    pub fn new() -> Self {
        Self::with_budget(MAX_RETOUCHED_BYTES)
    }

    pub fn with_budget(budget: usize) -> Self {
        Self {
            journal: StrokeJournal::new(),
            blocks: RetouchedBlocks::new(),
            blocks_bytes: 0,
            crop: None,
            histograms: None,
            budget,
        }
    }

    /// Bytes of retouched blocks and journal held for the next request.
    pub fn held_bytes(&self) -> usize {
        self.journal.bytes() + self.blocks_bytes
    }

    /// The histograms of `src` within `crop` with `strokes` laid over it. Only
    /// what differs from the last call is applied: strokes no longer asked for
    /// are rolled back, and for the same crop only the blocks that changed are
    /// binned again.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        passes: &RetouchPasses,
        src: &[u16],
        width: u32,
        height: u32,
        channels: usize,
        crop: &CropPayload,
        bins: &[u8],
        luts: &[Vec<f32>; 3],
        strokes: &[RetouchPayload],
    ) -> Result<Histograms, String> {
        let view = crop_view(crop, width, height);
        let binning = Binning { src, width, height, channels, view: &view, bins, luts };
        let mut counts = self.histograms.take().filter(|_| self.crop.as_ref() == Some(crop));

        let blocks = std::mem::take(&mut self.blocks);
        let mut source = LinearSource::with_blocks(src, width, height, channels, luts, blocks);
        // Each block that changes comes out of the counts as it was, and goes back in as it ends up.
        let mut changed = HashSet::new();
        let journal = &mut self.journal;
        let followed = follow_strokes(device, queue, passes, &mut source, journal, strokes, self.budget, |key, held| {
            if let Some(counts) = &mut counts {
                if changed.insert(key) {
                    binning.count_block(counts, key, held, false);
                }
            }
        });
        let histograms = match (followed, counts) {
            (Ok(Followed::Stepped), Some(mut counts)) => {
                for key in changed {
                    binning.count_block(&mut counts, key, source.block(key), true);
                }
                counts
            }
            (Ok(_), _) => {
                let mut counts = compute_histograms_in(src, width, height, channels, &view, bins);
                binning.rebin_retouched(&mut counts, &source);
                counts
            }
            (Err(e), _) => {
                *self = Self::with_budget(self.budget);
                return Err(e);
            }
        };
        if source.bytes() > self.budget {
            *self = Self::with_budget(self.budget);
            return Ok(histograms);
        }
        self.blocks_bytes = source.bytes();
        self.blocks = source.into_blocks();
        self.crop = Some(*crop);
        self.histograms = Some(histograms.clone());
        Ok(histograms)
    }
}

impl Default for RetouchedHistograms {
    fn default() -> Self {
        Self::new()
    }
}

/// R, G, B bins as u32 LE, end to end — the tail of the open payload.
pub fn histogram_bytes(h: &Histograms) -> Vec<u8> {
    h.r.iter()
        .chain(h.g.iter())
        .chain(h.b.iter())
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// Build the preview and overview textures and the histograms for a decoded
/// image.
///
/// Borrows rather than consumes, so the caller can move the full-resolution
/// buffer into the shared state afterwards without cloning it. `channels` is 3
/// for a source with no alpha channel and 4 for one with; both textures are RGBA
/// either way, since that is what the texture format requires.
pub fn prepare_image(
    src: &[u16],
    full_width: u32,
    full_height: u32,
    channels: usize,
    space: &SourceSpace,
) -> Result<PreparedImage, String> {
    // Histograms describe what is in the file, so they are binned off the
    // source code values before any conversion touches them.
    let histograms = compute_histograms(src, channels, &space.histogram_bins());

    // A profile that could not be split into curves and a matrix is applied
    // here in full. There are no separable channels for the darkroom block to
    // address in that case, so nothing is lost by converting early — every
    // other source keeps its own primaries until the GPU has run that block.
    //
    // A failure here cannot be swallowed: the rest of the pipeline is already
    // committed to the pixels being in the working space, so carrying on would
    // render the source's own encoding as if it were linear.
    let baked = space.bake(src, channels)?;
    let src: &[u16] = baked.as_deref().unwrap_or(src);
    // Resampling happens in linear light, so the image is gamma-decoded before
    // it is downscaled rather than after.
    //
    // Averaging sRGB code values averages the encoding, not the light. Reduce a
    // black-and-white checkerboard to a single pixel that way and it lands on
    // sRGB 128 — linear 0.21 — where the light those pixels stand for is linear
    // 0.50, sRGB 188. That is most of a stop, and it goes missing exactly in the
    // fine texture the preview exists to predict: foliage, hair, grain,
    // speculars. The live histogram reads the same preview, so it drifted too,
    // and the export renders full-resolution and never resampled at all — so
    // the two disagreed by more than resolution.
    //
    // Images already inside the budgets are not resampled, and take the direct path
    // so their bytes stay identical to what the export path would upload.
    //
    // A source small enough to skip both downscales needs no linear u16
    // intermediate at all; anything larger is resampled at least once, and the
    // decoded buffer is shared between the two.
    let long_edge = full_width.max(full_height);
    let resampled = long_edge > PREVIEW_MAX_EDGE
        || full_width as u64 * full_height as u64 > OVERVIEW_TARGET_PIXELS as u64;
    let linear = resampled.then(|| {
        let luts = space.luts(|v| (v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32).round() as u16);
        to_linear_u16(src, channels, &luts)
    });

    let (preview_width, preview_height, preview_pixels) = match &linear {
        Some(linear) if long_edge > PREVIEW_MAX_EDGE => {
            let (preview, w, h) =
                downscale_to_max_edge(linear, full_width, full_height, channels, PREVIEW_MAX_EDGE);
            (w, h, linear_u16_to_f16_bytes(&preview, channels))
        }
        _ => {
            let luts = space.luts(f16::from_f32);
            (full_width, full_height, linearize_u16(src, channels, &luts))
        }
    };

    // The overview always comes off the full frame, never off the preview, so it
    // keeps describing the whole image once the preview becomes a crop tile.
    let (overview_width, overview_height, overview_pixels) = match &linear {
        Some(linear) => {
            let (overview, w, h) = downscale_to_target_pixels(
                linear,
                full_width,
                full_height,
                channels,
                OVERVIEW_TARGET_PIXELS,
                OVERVIEW_MAX_EDGE,
            );
            (w, h, linear_u16_to_f16_bytes(&overview, channels))
        }
        None => (preview_width, preview_height, preview_pixels.clone()),
    };

    Ok(PreparedImage {
        preview_pixels,
        preview_width,
        preview_height,
        overview_pixels,
        overview_width,
        overview_height,
        full_width,
        full_height,
        histograms,
        to_working: space.to_working,
        baked: space.baked,
    })
}

/// Serialise a [`PreparedImage`] into the byte layout `openImage.ts` parses:
///
/// | offset | size          | contents                          |
/// |--------|---------------|-----------------------------------|
/// | 0      | 4             | preview width, u32 LE             |
/// | 4      | 4             | preview height, u32 LE            |
/// | 8      | 4             | full width, u32 LE                |
/// | 12     | 4             | full height, u32 LE               |
/// | 16     | 4             | overview width, u32 LE            |
/// | 20     | 4             | overview height, u32 LE           |
/// | 24     | 36            | source → working matrix, 9 f32 LE |
/// | 60     | 4             | flags, u32 LE: bit 0 = baked      |
/// | 64     | w × h × 8     | RGBA f16 LE preview pixels        |
/// | …      | ow × oh × 8   | RGBA f16 LE overview pixels       |
/// | …      | 3 × 256 × 4   | R, G, B histogram bins, u32 LE    |
pub fn build_payload(prepared: &PreparedImage) -> Vec<u8> {
    let hist_bytes = 3 * HIST_BINS * 4;
    let mut payload = Vec::with_capacity(
        PAYLOAD_HEADER_BYTES
            + prepared.preview_pixels.len()
            + prepared.overview_pixels.len()
            + hist_bytes,
    );

    payload.extend_from_slice(&prepared.preview_width.to_le_bytes());
    payload.extend_from_slice(&prepared.preview_height.to_le_bytes());
    payload.extend_from_slice(&prepared.full_width.to_le_bytes());
    payload.extend_from_slice(&prepared.full_height.to_le_bytes());
    payload.extend_from_slice(&prepared.overview_width.to_le_bytes());
    payload.extend_from_slice(&prepared.overview_height.to_le_bytes());
    for v in prepared.to_working {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    payload.extend_from_slice(&u32::from(prepared.baked).to_le_bytes());
    payload.extend_from_slice(&prepared.preview_pixels);
    payload.extend_from_slice(&prepared.overview_pixels);

    payload.extend_from_slice(&histogram_bytes(&prepared.histograms));

    payload
}

/// Expected payload length for a preview and overview of the given dimensions.
pub fn payload_len_for(
    preview_width: u32,
    preview_height: u32,
    overview_width: u32,
    overview_height: u32,
) -> usize {
    let pixels = |w: u32, h: u32| (w as usize) * (h as usize) * LINEAR_BYTES_PER_PIXEL;

    PAYLOAD_HEADER_BYTES
        + pixels(preview_width, preview_height)
        + pixels(overview_width, overview_height)
        + 3 * HIST_BINS * 4
}

/// Decode a file the `image` crate handles, returning its samples, channel
/// count, dimensions and what its profile says those samples mean.
fn decode_encoded(
    path: &std::path::Path,
) -> Result<(Vec<u16>, usize, u32, u32, SourceSpace), String> {
    let open_err = |e: image::ImageError| format!("Failed to open image: {e}");
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| format!("Failed to open image: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to open image: {e}"))?
        .into_decoder()
        .map_err(open_err)?;

    // The profile has to come off the decoder before building the image
    // consumes it. No profile means sRGB, which is what an untagged file means
    // in practice and what every file was assumed to be before this.
    let space = SourceSpace::resolve(decoder.icc_profile().ok().flatten());
    let img = image::DynamicImage::from_decoder(decoder).map_err(open_err)?;

    let (width, height) = (img.width(), img.height());

    // Sources whose format declares no alpha are kept three-wide. `into_rgb16` /
    // `into_rgba16` consume the DynamicImage, so the decoded source buffer is
    // released as soon as the conversion is done rather than idling.
    let (samples, channels) = if img.color().has_alpha() {
        (img.into_rgba16().into_raw(), 4)
    } else {
        (img.into_rgb16().into_raw(), 3)
    };

    Ok((samples, channels, width, height, space))
}

// ── Tauri command ─────────────────────────────────────────────────────────────

/// Error returned when the user dismisses the file picker.
///
/// Dismissal happens before the loaded image is released, so it is the one
/// failure that leaves the current image intact — `openImage.ts` matches on this
/// exact string to tell it apart from an open that got far enough to purge.
pub const OPEN_CANCELLED: &str = "open:cancelled";

#[tauri::command]
pub(crate) async fn open_image_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    image_id: u32,
) -> Result<Response, String> {
    println!("open_image_file: started");

    let mut extensions = vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()];
    extensions.extend(crate::raw::dialog_extensions());
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();

    let path = app
        .dialog()
        .file()
        .add_filter("Images", &extensions)
        .blocking_pick_file()
        .ok_or(OPEN_CANCELLED)?
        .into_path()
        .map_err(|e| e.to_string())?;

    // Release the previous image before decoding the next one. Held any longer
    // and two full-resolution buffers are resident at once, which for large
    // files roughly doubles the peak. The dialog has already returned a path, so
    // a cancelled open never reaches here and leaves the current image alone.
    {
        let mut guard = state.lock().unwrap();
        *guard = None;
    }

    let overall_start = Instant::now();

    // Sensor data takes its own decoder, and arrives already linear and in the
    // camera's primaries rather than carrying a profile to be read.
    let (samples, channels, full_width, full_height, space) = if crate::raw::is_raw_extension(&path)
    {
        let decoded = crate::raw::decode(&path)?;
        (
            decoded.pixels_u16,
            crate::raw::RAW_CHANNELS,
            decoded.width,
            decoded.height,
            decoded.space,
        )
    } else {
        decode_encoded(&path)?
    };

    let prepared = prepare_image(&samples, full_width, full_height, channels, &space)?;
    let metadata = crate::metadata::read(&path);

    // Store the original full-resolution image for export. This happens after
    // `prepare_image` so the buffer can be moved into the state rather than
    // cloned — preparation only needed to borrow it.
    {
        *RETOUCHED.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let mut guard = state.lock().unwrap();
        *guard = Some(OriginalImage {
            pixels_u16: Arc::new(samples),
            width: full_width,
            height: full_height,
            channels,
            space,
            full_resolution: Arc::default(),
            id: image_id,
            metadata,
        });
    }

    let payload = build_payload(&prepared);

    println!(
        "open_image_file: total backend time = {} ms, payload size = {} bytes, \
         colour converted on {}",
        overall_start.elapsed().as_millis(),
        payload.len(),
        if prepared.baked { "CPU" } else { "GPU" }
    );

    Ok(Response::new(payload))
}

/// How the open image was shot, or None once another image has replaced it.
#[tauri::command]
pub(crate) fn image_metadata(state: tauri::State<'_, ImageState>, image_id: u32) -> Option<ImageMetadata> {
    let guard = state.lock().unwrap();
    guard.as_ref().filter(|img| img.id == image_id).map(|img| img.metadata.clone())
}

/// The open image's per-channel histograms, binned over only what a crop
/// keeps, with any heal and clone strokes laid over it.
#[tauri::command]
pub(crate) async fn crop_histograms(
    state: tauri::State<'_, ImageState>,
    crop: CropPayload,
    retouch: Option<Vec<RetouchPayload>>,
) -> Result<Response, String> {
    let (pixels_u16, width, height, channels, bins, luts) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        let bins = img.space.histogram_bins();
        (Arc::clone(&img.pixels_u16), img.width, img.height, img.channels, bins, img.space.luts(|v| v))
    };

    let strokes = retouch.unwrap_or_default();
    // Strokes are laid over the image on the GPU, so they need a device first.
    let gpu = if strokes.is_empty() { None } else { Some(histogram_gpu().await?) };

    // One pass at a time: overlapping rayon jobs nest on the workers' stacks
    // while they wait on each other, and enough of them overflow it.
    static BINNING: Mutex<()> = Mutex::new(());
    let histograms = tauri::async_runtime::spawn_blocking(move || -> Result<Histograms, String> {
        let _binning = BINNING.lock().unwrap_or_else(|e| e.into_inner());
        let Some(gpu) = gpu else {
            let view = crop_view(&crop, width, height);
            return Ok(compute_histograms_in(&pixels_u16, width, height, channels, &view, &bins));
        };
        let mut cache = RETOUCHED.lock().unwrap_or_else(|e| e.into_inner());
        let held = cache.take().filter(|c| std::ptr::eq(c.image.as_ptr(), Arc::as_ptr(&pixels_u16)));
        let mut retouched = held.map_or_else(RetouchedHistograms::new, |c| c.histograms);
        let histograms = retouched.update(
            &gpu.device, &gpu.queue, &gpu.passes, &pixels_u16, width, height, channels, &crop, &bins, &luts, &strokes,
        )?;
        *cache = Some(Retouched { image: Arc::downgrade(&pixels_u16), histograms: retouched });
        Ok(histograms)
    })
    .await
    .map_err(|e| e.to_string())?;
    // A failed pass may mean the device was lost, so the next one starts afresh.
    if histograms.is_err() {
        *HISTOGRAM_GPU.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    Ok(Response::new(histogram_bytes(&histograms?)))
}

/// The device the retouched histograms are binned on, made the first time one is needed.
struct HistogramGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    passes: RetouchPasses,
}

static HISTOGRAM_GPU: Mutex<Option<Arc<HistogramGpu>>> = Mutex::new(None);

/// The retouched histograms of the image last binned with strokes.
struct Retouched {
    image: Weak<Vec<u16>>,
    histograms: RetouchedHistograms,
}

static RETOUCHED: Mutex<Option<Retouched>> = Mutex::new(None);

async fn histogram_gpu() -> Result<Arc<HistogramGpu>, String> {
    if let Some(gpu) = HISTOGRAM_GPU.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(gpu);
    }
    let (device, queue) = request_device("histogram_device").await?;
    let passes = RetouchPasses::new(&device);
    let gpu = Arc::new(HistogramGpu { device, queue, passes });
    // Two first requests may race; the device stored first is the one kept.
    Ok(Arc::clone(HISTOGRAM_GPU.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(gpu)))
}
