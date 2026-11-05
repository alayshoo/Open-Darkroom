import type { OutputSpaceId } from "$lib/gpu/outputSpace";

export type ExportFormat = "png" | "jpeg" | "webp" | "tiff" | "jxl";

export interface ExportSettings {
    format: ExportFormat;
    // The space the image is rendered into and the file is tagged with.
    outputSpace: OutputSpaceId;
    pngCompression: number;   // 0-9
    jpegQuality: number;      // 1-100
    webpLossless: boolean;
    webpQuality: number;      // 1-100 (lossy only)
    webpCompression: number;  // 0-9 (lossless only)
    tiffBitDepth: 8 | 16;
    jxlLossless: boolean;
    jxlQuality: number;       // 1-100 (lossy only), 90 is visually lossless
    jxlEffort: number;        // 1-9
    jxlBitDepth: number;      // 8-16 (lossless only; lossy is always 16)
    isBw: boolean;
}

export const defaultExportSettings: ExportSettings = {
    format: "jxl",
    outputSpace: "display-p3",
    pngCompression: 6,
    jpegQuality: 90,
    webpLossless: true,
    webpQuality: 90,
    webpCompression: 6,
    tiffBitDepth: 8,
    jxlLossless: false,
    jxlQuality: 95,
    jxlEffort: 7,
    jxlBitDepth: 8,
    isBw: false,
};
