The user is working on the development of a Svelte 5, Tauri 2 app called Open Darkroom. The goal of this app is to be an open source alternative to Adobe Lightroom and Negative Lab Pro. It focuses on Analog Photography workflows specifically mimicking the workflow of old Hasselblad scanners which invert negatives through the setting of black, white and gamma points for each RGB channel individually. However, the app could be used to edit regular photos as well, just like in Lightroom using Contrast, Saturation, Brightness and so on interfacing through sliders.

The user aims to use WebGPU for the image processing since it needs to be very snappy and real-time. The file handling will be done in the Rust backend and so will the conversion from RAW images into a format that will be handled by the WebGPU frontend.

The image data follows this path: 

The file is input on the rust side and is decoded into a 16 bit buffer since the maximum input image precision even in RAW images is 16 bit. On open, the backend sends the frontend a preview of at most 2048px, a small overview of the whole frame for the histogram, and the histograms, so the image shows straight away. The full resolution image then streams to the frontend in bands of rows, linearised to 16 bit float, into a single GPU texture whose mip levels are built on the GPU. An image larger than the GPU's texture limit is resized to fit. Once it is there, zooming and panning never go back to the backend.

The develop chain (develop.wgsl and the stages after it, in 16 bit float for the preview and 32 bit float on export) renders only the part of the image on screen plus a margin, at the resolution the zoom needs, over a low resolution backdrop of the whole image. A slider update re-renders through the chain; moving the view mostly re-runs only the final output transform. On Export the backend runs the same WGSL files, through wgpu, over the full resolution buffer. After transformations image is encoded and output as a file.

Don't build things without permission to do so, don't be too eager to build that you do so without asking the user first.

Do not litter the codebase with massive comment blocks. A comment larger than 3 lines in the middle of a file is way too much.
Comments should be generic, not based on your reasoning when solving a specific issue.

Never use ALL CAPS in the UI, whether typed that way or through `text-transform: uppercase`. Headings and labels are written in sentence case.

## Logging

The backend and the webview write to one log file per session in the OS log folder (`logging.rs`). The last ten files are kept, and the logs of the last three sessions that crashed are set aside where rotation cannot remove them. Only one copy of the app runs at a time. Never use `println!` or `console.*`: use the `log` macros in Rust and `createLogger("<scope>")` from `$lib/utils/log` in the frontend.

- `error`: a failure the user will notice, such as a failed open or export, a lost GPU device or a panic.
- `warn`: something that recovered or degraded, such as an image resized to the texture limit or a skipped preset.
- `info`: one line per lifecycle event, with the numbers needed to reproduce it: app start, GPU adapter, image opened (size, format, camera, time), full resolution ready, export (settings, size, time).
- `debug`: timings and per-step detail. Dropped unless Verbose logging is on in Settings, or in a debug build.

Never log per frame, per slider tick or per band at `info` or above. Log an error once, where it is handled: a command's `Err` is logged by the frontend code that catches it, with what the user was doing. Uncaught errors, rejections, panics, WebGPU uncaptured errors and device loss are already caught globally. Wrap anything that can repeat in `limitRepeats`. Paths may be logged as they are, since the home folder is replaced with `~`.

## Branches and releases

`main` is the default branch and the nightly build. All work branches off it and merges back into it. The `release` branch only receives `main` through the `release` skill, and every push to `release` builds the Windows `.msi` and a universal macOS `.dmg` and publishes them as a GitHub release. Never merge or push anything else into `release`.
