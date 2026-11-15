// $lib/library/imageOrder.ts
//
// How a roll's frames or a library's images are ordered. Stored on the roll or
// library as text: the key, with a leading `-` when descending.

import type { ImageRecord } from "./catalogApi";

export type OrderKey = "frame" | "name" | "date";
export interface ImageOrder {
    key: OrderKey;
    descending: boolean;
}

export const ORDER_KEYS: { key: OrderKey; label: string; rollsOnly?: boolean }[] = [
    { key: "frame", label: "Frame number", rollsOnly: true },
    { key: "name", label: "File name" },
    { key: "date", label: "Date taken" },
];

export const ROLL_DEFAULT: ImageOrder = { key: "frame", descending: false };
export const LIBRARY_DEFAULT: ImageOrder = { key: "name", descending: false };

export function parseOrder(text: string | undefined, fallback: ImageOrder, forRoll: boolean): ImageOrder {
    const descending = !!text?.startsWith("-");
    const key = (descending ? text!.slice(1) : text) as OrderKey;
    const known = ORDER_KEYS.find((k) => k.key === key);
    return known && (forRoll || !known.rollsOnly) ? { key, descending } : fallback;
}

export const serializeOrder = (order: ImageOrder) => `${order.descending ? "-" : ""}${order.key}`;

export function orderLabel(order: ImageOrder): string {
    const label = ORDER_KEYS.find((k) => k.key === order.key)?.label ?? "";
    return `${label}, ${directionLabels(order.key)[order.descending ? 1 : 0].toLowerCase()}`;
}

/** What ascending and descending mean for a key. */
export function directionLabels(key: OrderKey): [string, string] {
    if (key === "date") return ["Oldest first", "Newest first"];
    if (key === "frame") return ["First frame first", "Last frame first"];
    return ["A to Z", "Z to A"];
}

/** Compares names as people read them, so `2` comes before `10`. */
export const naturalCompare = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" }).compare;

/** When an image was taken, as EXIF has it, or else when it was added. */
function timeOf(image: ImageRecord): number {
    const m = image.metadata.dateTaken?.match(/^(\d{4}):(\d{2}):(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?/);
    if (!m) return image.added;
    const [, y, mo, d, h = "0", mi = "0", s = "0"] = m;
    const time = new Date(+y, +mo - 1, +d, +h, +mi, +s).getTime();
    return Number.isNaN(time) ? image.added : time;
}

function compareBy(key: OrderKey): (a: ImageRecord, b: ImageRecord) => number {
    if (key === "frame") return (a, b) => (a.frame ?? Infinity) - (b.frame ?? Infinity);
    if (key === "date") return (a, b) => timeOf(a) - timeOf(b);
    return (a, b) => naturalCompare(a.fileName, b.fileName);
}

/** A sorted copy. Ties fall back to the file name, then to when each was added. */
export function sortImages(images: ImageRecord[], order: ImageOrder): ImageRecord[] {
    const primary = compareBy(order.key);
    const sign = order.descending ? -1 : 1;
    const byName = compareBy("name");
    return [...images].sort(
        (a, b) => sign * primary(a, b) || sign * byName(a, b) || a.added - b.added || a.id - b.id,
    );
}
