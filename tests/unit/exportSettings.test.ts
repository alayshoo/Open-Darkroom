// tests/unit/exportSettings.test.ts
//
// A stored copy of the export settings is read back as far as it still makes
// sense. The size and file name rules live in the backend, in tests/export_options.rs.

import { describe, expect, it } from "vitest";
import { defaultExportSettings, restoreExportSettings } from "$lib/types/exportSettings";

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
