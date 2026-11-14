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
    | "lab"
    | "chemistry"
    | "dateShot"
    | "dateShotEnd"
    | "dateDeveloped"
    | "scanner"
    | "notes";

export type FrameField = "aperture" | "shutter" | "date" | "time" | "location" | "notes";

export interface FieldSpec<K extends string> {
    key: K;
    label: string;
    placeholder?: string;
    /** Half or a third of the row wide, so short values share a line. */
    half?: boolean;
    third?: boolean;
    multiline?: boolean;
    /** A fixed set of answers instead of free text. */
    options?: readonly string[];
    /** Makes the field a range, from its own key to this one. */
    until?: K;
    /** Shown only while this holds, so fields that do not apply stay out of the way. */
    when?: (info: RollInfo) => boolean;
}

const atLab = (info: RollInfo) => info.developedAt === "Lab";
const atHome = (info: RollInfo) => !atLab(info);

export const ROLL_SECTIONS: { title: string; fields: FieldSpec<RollField>[] }[] = [
    {
        title: "Film",
        fields: [
            { key: "filmStock", label: "Film stock", placeholder: "Kodak Portra 400" },
            { key: "format", label: "Format", placeholder: "35mm", third: true },
            { key: "boxIso", label: "Box ISO", placeholder: "400", third: true },
            { key: "ratedIso", label: "Rated at", placeholder: "800", third: true },
        ],
    },
    {
        title: "Dates",
        fields: [
            { key: "dateShot", label: "Shot", placeholder: "March 2024", until: "dateShotEnd" },
            { key: "dateDeveloped", label: "Developed", placeholder: "April 2024", half: true },
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
            { key: "lab", label: "Lab", when: atLab },
            { key: "chemistry", label: "Chemistry", placeholder: "Kodak Flexicolor", half: true, when: atLab },
            { key: "developer", label: "Developer", placeholder: "HC-110", when: atHome },
            { key: "dilution", label: "Dilution", placeholder: "1+31", half: true, when: atHome },
            { key: "devTime", label: "Time", placeholder: "6:30", half: true, when: atHome },
            { key: "devTemp", label: "Temperature", placeholder: "20 °C", half: true, when: atHome },
            { key: "pushPull", label: "Push / pull", placeholder: "+1", half: true },
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
    { key: "date", label: "Date", placeholder: "12 March 2024", half: true },
    { key: "time", label: "Time", placeholder: "14:30", half: true },
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
export function rollSummary(info: RollInfo, stockName = info.filmStock): string {
    const parts = [stockName, info.format].filter(Boolean) as string[];
    if (info.ratedIso && info.ratedIso !== info.boxIso) parts.push(`@ ${info.ratedIso}`);
    if (info.pushPull) parts.push(info.pushPull);
    return parts.join(" · ");
}
