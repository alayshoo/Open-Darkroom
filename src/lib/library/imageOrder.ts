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

const isDigit = (c: string) => c >= "0" && c <= "9";

function compareCodePoints(a: string, b: string): number {
    const [x, y] = [Array.from(a), Array.from(b)];
    for (let i = 0; i < Math.min(x.length, y.length); i++) {
        const order = x[i].codePointAt(0)! - y[i].codePointAt(0)!;
        if (order) return Math.sign(order);
    }
    return Math.sign(x.length - y.length);
}

/**
 * Compares names as people read them: case aside, and runs of digits by their
 * value, so `2` comes before `10`; leading zeros only settle a tie. Must match
 * natural_cmp in catalog.rs, which numbers frames on import.
 */
export function naturalCompare(a: string, b: string): number {
    const [x, y] = [Array.from(a), Array.from(b)];
    let [i, j, zeros] = [0, 0, 0];
    while (i < x.length && j < y.length) {
        if (isDigit(x[i]) && isDigit(y[j])) {
            const [si, sj] = [i, j];
            while (i < x.length && isDigit(x[i])) i++;
            while (j < y.length && isDigit(y[j])) j++;
            const [da, db] = [x.slice(si, i).join(""), y.slice(sj, j).join("")];
            const [ta, tb] = [da.replace(/^0+/, ""), db.replace(/^0+/, "")];
            const order = ta.length - tb.length || compareCodePoints(ta, tb);
            if (order) return Math.sign(order);
            zeros ||= Math.sign(da.length - db.length);
        } else {
            const order = compareCodePoints(x[i].toLowerCase(), y[j].toLowerCase());
            if (order) return order;
            i++;
            j++;
        }
    }
    return Number(i < x.length) - Number(j < y.length) || zeros;
}

/** When an image was taken, as EXIF has it, or else when it was added. */
function timeOf(image: ImageRecord): number {
    const m = image.metadata.dateTaken?.match(/^(\d{4}):(\d{2}):(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?/);
    if (!m) return image.added;
    const [, y, mo, d, h = "0", mi = "0", s = "0"] = m;
    const time = new Date(+y, +mo - 1, +d, +h, +mi, +s).getTime();
    return Number.isNaN(time) ? image.added : time;
}

/** A sorted copy. Ties fall back to the file name, then to when each was added. */
export function sortImages(images: ImageRecord[], order: ImageOrder): ImageRecord[] {
    const sign = order.descending ? -1 : 1;
    // Each image's key is worked out once, not on every comparison.
    const keyed = images.map((image) => ({
        image,
        value: order.key === "date" ? timeOf(image) : order.key === "frame" ? (image.frame ?? Number.MAX_SAFE_INTEGER) : 0,
    }));
    const byName = (a: ImageRecord, b: ImageRecord) => naturalCompare(a.fileName, b.fileName);
    return keyed
        .sort(
            (a, b) =>
                sign * (a.value - b.value) ||
                sign * byName(a.image, b.image) ||
                a.image.added - b.image.added ||
                a.image.id - b.image.id,
        )
        .map((k) => k.image);
}
