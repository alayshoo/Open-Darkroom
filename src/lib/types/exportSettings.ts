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

// The first is what a new file is given. Must match ExportFormat::extensions in export_encoding.rs.
export const FORMAT_EXTENSIONS: Record<ExportFormat, readonly string[]> = {
    png: ["png"],
    jpeg: ["jpg", "jpeg"],
    webp: ["webp"],
    tiff: ["tiff", "tif"],
    jxl: ["jxl"],
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

/** The dimensions a render is written at; never larger than it. Mirrors target_size in export_size.rs. */
export function targetSize(settings: ExportSettings, width: number, height: number): [number, number] {
    const pixels = Math.max(1, settings.resizePixels);
    let scale: number;
    switch (settings.resize) {
        case "longEdge": scale = pixels / Math.max(width, height); break;
        case "shortEdge": scale = pixels / Math.min(width, height); break;
        case "width": scale = pixels / width; break;
        case "height": scale = pixels / height; break;
        case "percent": scale = settings.resizePercent > 0 ? settings.resizePercent / 100 : 1; break;
        default: scale = 1;
    }
    if (!(scale < 1)) return [width, height];
    return [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
}

const FORBIDDEN = /[/\\:*?"<>|\p{Cc}]/u;

/** The file name an export is written under, or why it cannot be. Mirrors export_file_name in export.rs. */
export function exportFileName(name: string, format: ExportFormat): { name: string } | { error: string } {
    let stem = name.trim();
    const dot = stem.lastIndexOf(".");
    if (dot >= 0 && FORMAT_EXTENSIONS[format].includes(stem.slice(dot + 1).toLowerCase())) {
        stem = stem.slice(0, dot).trimEnd();
    }
    if (stem === "" || /^\.+$/.test(stem)) return { error: "The file needs a name" };
    const bad = stem.match(FORBIDDEN);
    if (bad) return { error: `A file name cannot contain ${JSON.stringify(bad[0])}` };
    if (stem.endsWith(".")) return { error: "A file name cannot end with a full stop" };
    return { name: `${stem}.${FORMAT_EXTENSIONS[format][0]}` };
}
