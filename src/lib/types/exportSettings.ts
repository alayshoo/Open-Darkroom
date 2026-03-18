export type ExportFormat = "png" | "jpeg" | "webp" | "tiff";

export interface ExportSettings {
    format: ExportFormat;
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
    pngCompression: 6,
    jpegQuality: 90,
    webpLossless: true,
    webpQuality: 90,
    webpCompression: 6,
    tiffBitDepth: 8,
    isBw: false,
};
