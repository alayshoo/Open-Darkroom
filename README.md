<div align="center">

<img src="resources/icon.png" alt="Open Darkroom" width="128" />

# Open Darkroom

**An open source darkroom for film scans and digital photos.**

Invert negatives the way a Hasselblad scanner does, with black, white and gamma points for each channel, then finish the image with Lightroom-style sliders. Everything renders on the GPU in real time.

[![Latest release](https://img.shields.io/github/v/release/alayshoo/Open-Darkroom?label=download&color=f5a623)](https://github.com/alayshoo/Open-Darkroom/releases/latest)
[![Tests](https://img.shields.io/github/actions/workflow/status/alayshoo/Open-Darkroom/tests.yml?branch=main&label=tests)](https://github.com/alayshoo/Open-Darkroom/actions/workflows/tests.yml)
![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-555)
![License](https://img.shields.io/badge/license-MIT-blue)

[Download](#download) · [Features](#features) · [Build from source](#build-from-source) · [How it works](#how-it-works)

</div>

---

## Why Open Darkroom?

Converting colour negatives usually means paying for Lightroom plus a plugin like Negative Lab Pro. Open Darkroom is a free, standalone alternative built around the scanner workflow: set the film base and the clip points per channel, and the conversion is yours to control rather than a black box.

It edits regular photos too, so a roll of film and a folder of digital shots go through the same tool.

## Features

### Negative inversion
- One-click **invert** for colour and black and white negatives
- **Black point, gamma and white point** for red, green and blue individually, plus a master set over all three
- Per-channel histograms behind the level sliders
- **Auto levels** to find the black and white points for you

### Develop
- **Light:** exposure, contrast, brightness, highlights, shadows, whites, blacks
- **Colour:** white balance (temperature and tint), saturation, vibrance, hue
- **HSL:** hue, saturation and luminance across eight colour bands
- **Curves** editor
- **Detail:** clarity, texture and unsharp masking with luma and detail thresholds

### Workflow
- **RAW support** for the cameras [rawler](https://github.com/dnglab/dnglab) reads, alongside JPEG and PNG
- **Crop** with preset and custom aspect ratios, plus horizontal and vertical flips
- **Presets**, and copy and paste of adjustments between images
- Full **undo and redo** history
- **Loupe** and smooth zoom and pan over the full resolution image
- Rebindable **keyboard shortcuts**

### Colour management
- A wide-gamut **Rec. 2020** working space
- Export to **sRGB, Display P3, Adobe RGB or Rec. 2020**, with the ICC profile embedded
- Soft proofing and a gamut overlay to see what an output space will clip

### Export
| Format | Options |
| --- | --- |
| **TIFF** | 8 or 16 bit |
| **PNG** | Compression level |
| **JPEG** | Quality |
| **WebP** | Lossy or lossless |

Exports run the same shaders as the preview, at full resolution in 32-bit float, so what you see is what you get.

## Download

Grab the latest build from the [**Releases**](https://github.com/alayshoo/Open-Darkroom/releases/latest) page.

| Platform | File | Notes |
| --- | --- | --- |
| **Windows** | `.msi` | Windows may warn the installer is unrecognised: choose *More info*, then *Run anyway*. |
| **macOS** | `.dmg` | Universal, runs on Apple Silicon and Intel. The app is not signed by Apple, so on first launch open *System Settings → Privacy & Security* and choose *Open Anyway*. |

Open Darkroom needs a GPU with **WebGPU** support.

## Build from source

### Prerequisites
- [Node.js](https://nodejs.org/) 22 and [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

### Run it

```bash
pnpm install
```

```bash
pnpm tauri dev
```

### Package an installer

```bash
pnpm tauri build
```

### Test

```bash
pnpm test:unit
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

## How it works

```mermaid
flowchart LR
    A[RAW / JPEG / PNG] --> B[Rust backend<br/>decode to 16-bit]
    B -->|preview + histograms| C[Svelte frontend]
    B -->|full resolution, streamed in bands| D[GPU texture<br/>with mip levels]
    D --> E[WGSL develop chain<br/>WebGPU, 16-bit float]
    C --> E
    E --> F[Screen]
    B --> G[Same WGSL via wgpu<br/>32-bit float]
    G --> H[Encoded file + ICC]
```

- **Opening:** the Rust backend decodes the file into a 16-bit buffer and sends a preview of up to 2048px along with the histograms, so the image appears straight away. The full resolution image then streams to the GPU in bands of rows.
- **Editing:** the develop chain is written in WGSL and runs on WebGPU. It renders only what is on screen, at the resolution the zoom needs, so sliders stay real time on large scans. Zooming and panning never go back to the backend.
- **Exporting:** the backend runs the very same WGSL files through wgpu over the full resolution buffer, then encodes the result.

### Built with

[Tauri 2](https://tauri.app/) · [Svelte 5](https://svelte.dev/) · [WebGPU](https://www.w3.org/TR/webgpu/) · [wgpu](https://wgpu.rs/) · [rawler](https://github.com/dnglab/dnglab) · [Little CMS](https://www.littlecms.com/) · [Tailwind CSS](https://tailwindcss.com/)

## Contributing

Issues and pull requests are welcome. Branch off `main`, which is the nightly build, and open your pull request back into it. The `release` branch is only updated from `main` when a new version ships.

## License

Released under the [MIT License](https://opensource.org/license/mit).
