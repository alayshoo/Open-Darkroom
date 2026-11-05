import { WORKING_SPACE, type OutputSpaceId } from "$lib/gpu/outputSpace";

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
    jxlBitDepth: number;      // 8-16
    isBw: boolean;
}

export const defaultExportSettings: ExportSettings = {
    format: "png",
    outputSpace: "srgb",
    pngCompression: 6,
    jpegQuality: 90,
    webpLossless: true,
    webpQuality: 90,
    webpCompression: 6,
    tiffBitDepth: 8,
    jxlLossless: false,
    jxlQuality: 99,
    jxlEffort: 7,
    jxlBitDepth: 16,
    isBw: false,
};

/** Bits per channel the chosen format is written at. */
export function exportBitDepth(settings: ExportSettings): number {
    if (settings.format === "tiff") return settings.tiffBitDepth;
    if (settings.format === "jxl") return settings.jxlBitDepth;
    return 8;
}

/**
 * The space a format is written in unless the choice is overridden.
 *
 * Eight bits is for sending somewhere, where sRGB is the only space that can be
 * relied on to be read correctly. More is for keeping, and spending that
 * precision on sRGB's gamut would throw away most of what the working space was
 * holding — so a 16-bit file defaults to the working space itself, which is the
 * one destination that converts nothing at all.
 */
export function defaultOutputSpace(bitDepth: number): OutputSpaceId {
    return bitDepth > 8 ? WORKING_SPACE : "srgb";
}
