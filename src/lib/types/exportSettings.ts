import { OUTPUT_SPACE_IDS, type OutputSpaceId } from "$lib/gpu/outputSpace";

export type ExportFormat = "png" | "jpeg" | "webp" | "tiff" | "jxl";

export const EXPORT_FORMATS: readonly ExportFormat[] = ["jxl", "jpeg", "png", "tiff", "webp"];

export const FORMAT_LABELS: Record<ExportFormat, string> = {
    jxl: "JPEG XL",
    jpeg: "JPEG",
    png: "PNG",
    tiff: "TIFF",
    webp: "WebP",
};

// Shown beside the name field; the backend decides the name a file is written under.
export const FORMAT_EXTENSION: Record<ExportFormat, string> = {
    png: "png",
    jpeg: "jpg",
    webp: "webp",
    tiff: "tiff",
    jxl: "jxl",
};

export type ResizeMode = "none" | "longEdge" | "shortEdge" | "width" | "height" | "percent";

export const RESIZE_MODES: readonly ResizeMode[] = ["none", "longEdge", "shortEdge", "width", "height", "percent"];

export const RESIZE_LABELS: Record<ResizeMode, string> = {
    none: "Full size",
    longEdge: "Long edge",
    shortEdge: "Short edge",
    width: "Width",
    height: "Height",
    percent: "Percentage",
};

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
    resize: ResizeMode;
    resizePixels: number;     // the edge length for every mode but "percent"
    resizePercent: number;    // 1-100
    // Whether the camera, lens, exposure and date are written into the file.
    metadata: boolean;
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
    jxlBitDepth: 16,
    isBw: false,
    resize: "none",
    resizePixels: 2048,
    resizePercent: 50,
    metadata: true,
};

const RANGES: Partial<Record<keyof ExportSettings, [number, number]>> = {
    pngCompression: [0, 9],
    jpegQuality: [1, 100],
    webpQuality: [1, 100],
    webpCompression: [0, 9],
    jxlQuality: [1, 100],
    jxlEffort: [1, 9],
    jxlBitDepth: [8, 16],
    resizePixels: [1, 65535],
    resizePercent: [1, 100],
};

const CHOICES: Partial<Record<keyof ExportSettings, readonly unknown[]>> = {
    format: EXPORT_FORMATS,
    outputSpace: OUTPUT_SPACE_IDS,
    tiffBitDepth: [8, 16],
    resize: RESIZE_MODES,
};

/** As much of a stored copy as still makes sense, over the defaults. */
export function restoreExportSettings(stored: unknown): ExportSettings {
    const out = { ...defaultExportSettings };
    if (typeof stored !== "object" || stored === null) return out;
    const record = out as unknown as Record<string, unknown>;
    for (const [key, value] of Object.entries(stored)) {
        const k = key as keyof ExportSettings;
        if (!(k in defaultExportSettings) || k === "isBw") continue;
        if (typeof value !== typeof defaultExportSettings[k]) continue;
        const choices = CHOICES[k];
        if (choices && !choices.includes(value)) continue;
        const range = RANGES[k];
        if (range && !(Number.isInteger(value) && (value as number) >= range[0] && (value as number) <= range[1])) continue;
        record[k] = value;
    }
    return out;
}
