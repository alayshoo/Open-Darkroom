// tests/unit/exportSettings.test.ts
//
// What the export modal promises before it hands over to the backend: the size
// and the file name it shows must be the ones export_size.rs and export.rs
// write, so these cases are the same as tests/export_options.rs. A stored copy
// of the settings is read back as far as it still makes sense.

import { describe, expect, it } from "vitest";
import {
    defaultExportSettings,
    exportFileName,
    restoreExportSettings,
    targetSize,
    type ExportSettings,
    type ResizeMode,
} from "$lib/types/exportSettings";

const sized = (resize: ResizeMode, resizePixels: number, resizePercent = 50): ExportSettings => ({
    ...defaultExportSettings,
    resize,
    resizePixels,
    resizePercent,
});

describe("targetSize", () => {
    it("lands each mode's edge on the number asked for", () => {
        expect(targetSize(sized("none", 100), 6000, 4000)).toEqual([6000, 4000]);
        expect(targetSize(sized("longEdge", 2048), 6000, 4000)).toEqual([2048, 1365]);
        expect(targetSize(sized("longEdge", 2048), 4000, 6000)).toEqual([1365, 2048]);
        expect(targetSize(sized("shortEdge", 1080), 6000, 4000)).toEqual([1620, 1080]);
        expect(targetSize(sized("width", 1200), 6000, 4000)).toEqual([1200, 800]);
        expect(targetSize(sized("height", 1000), 6000, 4000)).toEqual([1500, 1000]);
        expect(targetSize(sized("percent", 0, 25), 6000, 4000)).toEqual([1500, 1000]);
    });

    it("never enlarges", () => {
        expect(targetSize(sized("longEdge", 4000), 800, 600)).toEqual([800, 600]);
        expect(targetSize(sized("percent", 0, 150), 800, 600)).toEqual([800, 600]);
        expect(targetSize(sized("percent", 0, Number.NaN), 800, 600)).toEqual([800, 600]);
    });

    it("keeps at least a pixel", () => {
        expect(targetSize(sized("longEdge", 10), 10_000, 3)).toEqual([10, 1]);
    });
});

describe("exportFileName", () => {
    it("takes the format's extension once", () => {
        expect(exportFileName("IMG_0042", "jxl")).toEqual({ name: "IMG_0042.jxl" });
        expect(exportFileName("  roll 3  ", "png")).toEqual({ name: "roll 3.png" });
        expect(exportFileName("frame.JPEG", "jpeg")).toEqual({ name: "frame.jpg" });
        expect(exportFileName("frame.tif", "tiff")).toEqual({ name: "frame.tiff" });
        expect(exportFileName("v1.2", "webp")).toEqual({ name: "v1.2.webp" });
    });

    it.each(["", "   ", ".jxl", "..", "a/b", "a\\b", "a:b", "what?", "dot.", "tab\there"])(
        "refuses %j",
        (bad) => {
            expect(exportFileName(bad, "jxl")).toHaveProperty("error");
        },
    );
});

describe("restoreExportSettings", () => {
    it("falls back to the defaults for anything that is not an object", () => {
        expect(restoreExportSettings(null)).toEqual(defaultExportSettings);
        expect(restoreExportSettings("jpeg")).toEqual(defaultExportSettings);
    });

    it("keeps what is valid and drops what is not", () => {
        const restored = restoreExportSettings({
            format: "jpeg",
            jpegQuality: 72,
            outputSpace: "adobe-rgb",
            resize: "longEdge",
            resizePixels: 3000,
            metadata: false,
            // Out of range, of the wrong type, unknown, or not a choice.
            jxlEffort: 12,
            pngCompression: "6",
            tiffBitDepth: 12,
            colour: "red",
            webpQuality: 50.5,
        });
        expect(restored).toEqual({
            ...defaultExportSettings,
            format: "jpeg",
            jpegQuality: 72,
            outputSpace: "adobe-rgb",
            resize: "longEdge",
            resizePixels: 3000,
            metadata: false,
        });
    });

    it("leaves the greyscale flag to the image being exported", () => {
        expect(restoreExportSettings({ isBw: true }).isBw).toBe(false);
    });
});
