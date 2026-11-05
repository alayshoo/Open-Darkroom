# Changelog

All notable changes to Open Darkroom are listed here, newest first. Each release is tagged `v<version>` and its installers are on the [releases page](https://github.com/alayshoo/Open-Darkroom/releases).

Versions follow [semantic versioning](https://semver.org): major for big or breaking changes, minor for new features, patch for fixes only.

## [1.1.0] - 2026-11-05

### Added
- Heal and clone brushes for dust and scratches, taken up with X and C. The stroke paints into the image as you draw it. Each brush finds its own source unless you Alt-click to pick one, and Alt-drag sizes and softens it. Size, feather and opacity sit in a bar along the foot of the window.
- Retouch strokes show in the histograms and are applied on export.
- JPEG XL export, lossless or lossy, at 8 to 16 bits.
- The colour space panel (F3) colours its chromaticity cloud by how much of the image falls in each spot, with a legend. Each gamut can stripe the pixels outside it on the preview.

### Changed
- The export progress bar now runs through Preparing, Rendering and Encoding. It used to sit at 100% while the file was written.
- Gamut triangles in the colour space panel follow your display's colour space rather than always sRGB.
- The crop tools fade the other tools out instead of pushing them down, and the side bar slides away more smoothly.

### Fixed
- A failed export no longer leaves a partial file behind or overwrites the file it would have replaced.
- Exporting uses less memory, most of all for JPEG XL.
- The title bar stays usable during an export, and keyboard shortcuts no longer fire while one runs.

## [1.0.1] - 2026-11-03

### Fixed
- Dragging a crop no longer crashes the app while the histograms catch up with it.

## [1.0.0] - 2026-11-03

The first release.

### Added
- Automatic negative inversion with a black point, white point and gamma for each colour channel, set from the histograms of what the crop keeps.
- Develop tools: exposure, contrast, brightness, highlights, shadows, whites and blacks; white balance, saturation, vibrance and hue; HSL across eight colour bands; curves; clarity, texture and sharpening.
- RAW, JPEG and PNG support.
- Crop with preset and custom aspect ratios, and horizontal and vertical flips.
- Presets, copy and paste of adjustments, and undo and redo.
- Loupe, zoom and pan over the full resolution image.
- Rebindable keyboard shortcuts.
- A wide-gamut Rec. 2020 working space, with soft proofing and a gamut overlay.
- Export to TIFF, PNG, JPEG and WebP in sRGB, Display P3, Adobe RGB or Rec. 2020, with the ICC profile embedded.
- Windows `.msi` and universal macOS `.dmg` installers.

[1.1.0]: https://github.com/alayshoo/Open-Darkroom/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/alayshoo/Open-Darkroom/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/alayshoo/Open-Darkroom/releases/tag/v1.0.0
