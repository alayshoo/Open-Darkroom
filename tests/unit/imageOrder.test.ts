// tests/unit/imageOrder.test.ts
//
// How a roll's frames and a library's images are ordered, and how that order is stored.

import { describe, expect, it } from "vitest";

import type { ImageRecord } from "$lib/library/catalogApi";
import { LIBRARY_DEFAULT, ROLL_DEFAULT, naturalCompare, parseOrder, serializeOrder, sortImages } from "$lib/library/imageOrder";
import naturalOrder from "../fixtures/naturalOrder.json";

let nextId = 1;
function image(fileName: string, fields: { frame?: number; dateTaken?: string; added?: number } = {}): ImageRecord {
    return {
        id: nextId++,
        libraryId: 1,
        rollId: fields.frame === undefined ? null : 1,
        frame: fields.frame ?? null,
        fileName,
        path: `/scans/${fileName}`,
        inPhotoFolder: false,
        missing: false,
        metadata: {
            fileName, make: null, model: null, lens: null, focalLength: null,
            aperture: null, exposureTime: null, iso: null, exposureBias: null, dateTaken: fields.dateTaken ?? null,
        },
        frameInfo: {},
        edited: null,
        added: fields.added ?? 0,
    };
}

const names = (images: ImageRecord[]) => images.map((i) => i.fileName);

describe("naturalCompare", () => {
    it("orders names the way the backend numbers frames on import", () => {
        // The same list src-tauri/tests/catalog.rs checks natural_cmp against.
        expect([...naturalOrder].reverse().sort(naturalCompare)).toEqual(naturalOrder);
    });
});

describe("sortImages", () => {
    it("orders file names by the numbers in them", () => {
        const images = ["10.tif", "2.tif", "1.tif", "Scan 11.tif", "scan 9.tif"].map((n) => image(n));
        expect(names(sortImages(images, { key: "name", descending: false }))).toEqual([
            "1.tif", "2.tif", "10.tif", "scan 9.tif", "Scan 11.tif",
        ]);
        expect(names(sortImages(images, { key: "name", descending: true }))).toEqual([
            "Scan 11.tif", "scan 9.tif", "10.tif", "2.tif", "1.tif",
        ]);
    });

    it("orders by date taken, falling back to when an image was added", () => {
        const images = [
            image("a.tif", { dateTaken: "2024:03:12 14:03:00" }),
            image("b.tif", { dateTaken: "2023:11:02 09:00:00" }),
            image("c.tif", { added: new Date(2024, 0, 1).getTime() }),
        ];
        expect(names(sortImages(images, { key: "date", descending: false }))).toEqual(["b.tif", "c.tif", "a.tif"]);
        expect(names(sortImages(images, { key: "date", descending: true }))).toEqual(["a.tif", "c.tif", "b.tif"]);
    });

    it("orders frames by number", () => {
        const images = [image("x.tif", { frame: 3 }), image("y.tif", { frame: 1 }), image("z.tif", { frame: 2 })];
        expect(names(sortImages(images, ROLL_DEFAULT))).toEqual(["y.tif", "z.tif", "x.tif"]);
    });
});

describe("parseOrder", () => {
    it("reads back what it wrote, and falls back on anything else", () => {
        const order = { key: "date", descending: true } as const;
        expect(parseOrder(serializeOrder(order), LIBRARY_DEFAULT, false)).toEqual(order);
        expect(parseOrder("", ROLL_DEFAULT, true)).toEqual(ROLL_DEFAULT);
        expect(parseOrder("shuffle", LIBRARY_DEFAULT, false)).toEqual(LIBRARY_DEFAULT);
        expect(parseOrder("frame", LIBRARY_DEFAULT, false)).toEqual(LIBRARY_DEFAULT);
    });
});
