export type ExportFormat = "png" | "jpeg";

export interface ExportSettings {
    format: ExportFormat;
    pngCompression: number; // 0–9
    jpegQuality: number;    // 1–100
}

export const defaultExportSettings: ExportSettings = {
    format: "png",
    pngCompression: 6,
    jpegQuality: 90,
};
