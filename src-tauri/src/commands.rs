// src-tauri/src/commands.rs
//
// The Tauri command handlers for opening an image and serving it to the
// frontend. The work itself lives in image_opening and full_resolution, which
// know nothing of Tauri; these only bridge them to the dialog and the IPC.

use std::sync::Arc;
use std::time::Instant;
use tauri::ipc::Response;
use tauri_plugin_dialog::DialogExt;

use crate::crop::{crop_view, CropPayload};
use crate::full_resolution::build_rows_payload;
use crate::image_opening::{
    build_payload, compute_histograms_in, decode_file, histogram_bytes, open_extensions,
    prepare_image, ImageState, OriginalImage, OPEN_CANCELLED,
};

#[tauri::command]
pub(crate) async fn open_image_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, ImageState>,
    image_id: u32,
) -> Result<Response, String> {
    println!("open_image_file: started");

    let extensions = open_extensions();
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

    let (samples, channels, full_width, full_height, space) = decode_file(&path)?;

    let prepared = prepare_image(&samples, full_width, full_height, channels, &space)?;

    // Store the original full-resolution image for export. This happens after
    // `prepare_image` so the buffer can be moved into the state rather than
    // cloned — preparation only needed to borrow it.
    {
        let mut guard = state.lock().unwrap();
        *guard = Some(OriginalImage {
            pixels_u16: Arc::new(samples),
            width: full_width,
            height: full_height,
            channels,
            space,
            full_resolution: Arc::default(),
            id: image_id,
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

/// The open image's per-channel histograms, binned over only what a crop keeps.
#[tauri::command]
pub(crate) async fn crop_histograms(
    state: tauri::State<'_, ImageState>,
    crop: CropPayload,
) -> Result<Response, String> {
    let (pixels_u16, width, height, channels) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        (Arc::clone(&img.pixels_u16), img.width, img.height, img.channels)
    };

    let view = crop_view(&crop, width, height);
    let histograms = compute_histograms_in(&pixels_u16, width, height, channels, &view);
    Ok(Response::new(histogram_bytes(&histograms)))
}

/// A band of rows of the open image, for a GPU whose textures reach `max_edge`.
#[tauri::command]
pub(crate) async fn full_resolution_rows(
    state: tauri::State<'_, ImageState>,
    image_id: u32,
    max_edge: u32,
    y: u32,
    rows: u32,
) -> Result<Response, String> {
    let (pixels, width, height, channels, space, full) = {
        let guard = state.lock().unwrap();
        let img = guard.as_ref().ok_or("No image loaded")?;
        if img.id != image_id {
            return Err("The image has been replaced since this band was asked for".into());
        }
        (
            Arc::clone(&img.pixels_u16),
            img.width,
            img.height,
            img.channels,
            img.space.clone(),
            Arc::clone(&img.full_resolution),
        )
    };

    // Off the async runtime: converting a band, and above all resizing the
    // whole image the first time a fitted one is asked for, would stall it.
    let payload = tauri::async_runtime::spawn_blocking(move || {
        full.rows(&pixels, width, height, channels, &space, max_edge, y, rows)
            .map(|r| build_rows_payload(&r))
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(Response::new(payload))
}
