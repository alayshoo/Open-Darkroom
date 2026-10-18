import { WORKING_SPACE, type OutputSpaceId } from "$lib/gpu/outputSpace";

export type ExportFormat = "png" | "jpeg" | "webp" | "tiff";

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
    isBw: false,
};

/**
 * The space a format is written in unless the choice is overridden.
 *
 * Eight bits is for sending somewhere, where sRGB is the only space that can be
 * relied on to be read correctly. Sixteen is for keeping, and spending that
 * precision on sRGB's gamut would throw away most of what the working space was
 * holding — so a 16-bit TIFF defaults to the working space itself, which is the
 * one destination that converts nothing at all.
 */
export function defaultOutputSpace(
    format: ExportFormat,
    tiffBitDepth: 8 | 16,
): OutputSpaceId {
    return format === "tiff" && tiffBitDepth === 16 ? WORKING_SPACE : "srgb";
}
