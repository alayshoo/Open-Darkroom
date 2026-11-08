// $lib/types/rollInfo.ts
//
// How a roll was shot and developed, and what is logged against each frame.
// Every field is optional free text, so a roll can be filled in at any time.

import type { ImageMetadata } from "./imageMetadata";
import { formatAperture, formatShutter } from "./imageMetadata";

export type RollInfo = Partial<Record<RollField, string>>;
export type FrameInfo = Partial<Record<FrameField, string>>;

export type RollField =
    | "filmStock"
    | "format"
    | "boxIso"
    | "ratedIso"
    | "pushPull"
    | "camera"
    | "lenses"
    | "developer"
    | "dilution"
    | "devTime"
    | "devTemp"
    | "developedAt"
    | "dateShot"
    | "dateDeveloped"
    | "scanner"
    | "notes";

export type FrameField = "aperture" | "shutter" | "location" | "notes";

export interface FieldSpec<K extends string> {
    key: K;
    label: string;
    placeholder?: string;
    /** Half the row wide, so short values sit two to a line. */
    half?: boolean;
    multiline?: boolean;
    /** A fixed set of answers instead of free text. */
    options?: readonly string[];
}

export const ROLL_SECTIONS: { title: string; fields: FieldSpec<RollField>[] }[] = [
    {
        title: "Film",
        fields: [
            { key: "filmStock", label: "Film stock", placeholder: "Kodak Portra 400" },
            { key: "format", label: "Format", placeholder: "35mm", half: true },
            { key: "pushPull", label: "Push / pull", placeholder: "+1", half: true },
            { key: "boxIso", label: "Box ISO", placeholder: "400", half: true },
            { key: "ratedIso", label: "Rated at", placeholder: "800", half: true },
        ],
    },
    {
        title: "Camera",
        fields: [
            { key: "camera", label: "Camera" },
            { key: "lenses", label: "Lenses" },
        ],
    },
    {
        title: "Development",
        fields: [
            { key: "developedAt", label: "Developed", options: ["Home", "Lab"] },
            { key: "developer", label: "Developer", placeholder: "HC-110" },
            { key: "dilution", label: "Dilution", placeholder: "1+31", half: true },
            { key: "devTime", label: "Time", placeholder: "6:30", half: true },
            { key: "devTemp", label: "Temperature", placeholder: "20 °C", half: true },
        ],
    },
    {
        title: "Dates",
        fields: [
            { key: "dateShot", label: "Shot", placeholder: "March 2024", half: true },
            { key: "dateDeveloped", label: "Developed", placeholder: "April 2024", half: true },
        ],
    },
    {
        title: "Scan",
        fields: [
            { key: "scanner", label: "Scanner", placeholder: "Hasselblad Flextight X1" },
            { key: "notes", label: "Notes", multiline: true },
        ],
    },
];

export const FRAME_FIELDS: FieldSpec<FrameField>[] = [
    { key: "aperture", label: "Aperture", placeholder: "f/8", half: true },
    { key: "shutter", label: "Shutter", placeholder: "1/125 s", half: true },
    { key: "location", label: "Location" },
    { key: "notes", label: "Notes", multiline: true },
];

/** What the file itself says for a frame field, offered until one is logged. */
export function framePlaceholder(key: FrameField, metadata: ImageMetadata | null): string | undefined {
    if (key === "aperture" && metadata?.aperture) return formatAperture(metadata.aperture);
    if (key === "shutter" && metadata?.exposureTime) return formatShutter(metadata.exposureTime);
    return FRAME_FIELDS.find((f) => f.key === key)?.placeholder;
}

/** One line about a roll for its card: the stock, and how it was rated if not at box speed. */
export function rollSummary(info: RollInfo): string {
    const parts = [info.filmStock, info.format].filter(Boolean) as string[];
    if (info.ratedIso && info.ratedIso !== info.boxIso) parts.push(`@ ${info.ratedIso}`);
    if (info.pushPull) parts.push(info.pushPull);
    return parts.join(" · ");
}
