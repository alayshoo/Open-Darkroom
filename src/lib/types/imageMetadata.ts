// $lib/types/imageMetadata.ts
//
// How an image was shot, as the backend reads it from the file, and how the
// info panel writes it out. Must match ImageMetadata in metadata.rs.

export interface ImageMetadata {
    fileName: string;
    make: string | null;
    model: string | null;
    lens: string | null;
    /** In millimetres. */
    focalLength: number | null;
    /** The f-number. */
    aperture: number | null;
    /** In seconds, as the fraction the camera recorded. */
    exposureTime: [number, number] | null;
    iso: number | null;
    /** In stops. */
    exposureBias: number | null;
    /** As EXIF writes it: `YYYY:MM:DD HH:MM:SS`. */
    dateTaken: string | null;
}

export interface MetadataRow {
    label: string;
    value: string;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** At most `places` decimals, with trailing zeros dropped. */
const trimmed = (v: number, places: number) => String(Number(v.toFixed(places)));

// Most models carry the make already ("Canon EOS R5"); some do not ("X-T3").
export function formatCamera(make: string | null, model: string | null): string | null {
    if (!model) return make;
    if (!make) return model;
    const brand = make.split(/\s+/)[0].toLowerCase();
    return model.toLowerCase().startsWith(brand) ? model : `${make} ${model}`;
}

export function formatShutter([n, d]: [number, number]): string {
    const seconds = n / d;
    if (seconds >= 0.3) return `${trimmed(seconds, 1)} s`;
    return `1/${Math.round(d / n)} s`;
}

export const formatAperture = (f: number) => `f/${trimmed(f, 1)}`;

export const formatFocalLength = (mm: number) => `${trimmed(mm, mm < 10 ? 1 : 0)} mm`;

export function formatExposureBias(ev: number): string {
    const rounded = Number(ev.toFixed(1));
    if (rounded === 0) return "0 EV";
    return `${rounded > 0 ? "+" : "−"}${Math.abs(rounded)} EV`;
}

/** `12 Mar 2024, 14:03`, or the text as written when it is not a date EXIF writes. */
export function formatDateTaken(exif: string): string {
    const m = exif.match(/^(\d{4}):(\d{2}):(\d{2})(?:[ T](\d{2}):(\d{2}))?/);
    const month = m ? MONTHS[Number(m[2]) - 1] : undefined;
    if (!m || !month || m[1] === "0000") return exif;
    const date = `${Number(m[3])} ${month} ${m[1]}`;
    return m[4] ? `${date}, ${m[4]}:${m[5]}` : date;
}

/** What the info panel lists, leaving out anything the file does not say. */
export function metadataRows(meta: ImageMetadata, width: number, height: number): MetadataRow[] {
    const rows: [string, string | null][] = [
        ["File", meta.fileName || null],
        ["Dimensions", `${width} × ${height}`],
        ["Camera", formatCamera(meta.make, meta.model)],
        ["Lens", meta.lens],
        ["Focal length", meta.focalLength === null ? null : formatFocalLength(meta.focalLength)],
        ["Aperture", meta.aperture === null ? null : formatAperture(meta.aperture)],
        ["Shutter", meta.exposureTime === null ? null : formatShutter(meta.exposureTime)],
        ["ISO", meta.iso === null ? null : String(meta.iso)],
        ["Exposure", meta.exposureBias === null ? null : formatExposureBias(meta.exposureBias)],
        ["Taken", meta.dateTaken === null ? null : formatDateTaken(meta.dateTaken)],
    ];
    return rows.filter((r): r is [string, string] => r[1] !== null).map(([label, value]) => ({ label, value }));
}

/** Whether the file said anything about how it was shot, beyond its name and size. */
export function hasShootingInfo(meta: ImageMetadata): boolean {
    return [meta.make, meta.model, meta.lens, meta.focalLength, meta.aperture, meta.exposureTime, meta.iso, meta.exposureBias, meta.dateTaken]
        .some((v) => v !== null);
}
