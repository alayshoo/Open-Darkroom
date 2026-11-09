// tests/unit/imageMetadata.test.ts
//
// How the info panel writes out what a file says about how it was shot.

import { describe, expect, it } from "vitest";
import {
    type ImageMetadata,
    formatAperture,
    formatCamera,
    formatDateTaken,
    formatExposureBias,
    formatFocalLength,
    formatShutter,
    hasShootingInfo,
    metadataRows,
} from "$lib/types/imageMetadata";

const NOTHING: ImageMetadata = {
    fileName: "scan.png",
    make: null,
    model: null,
    lens: null,
    focalLength: null,
    aperture: null,
    exposureTime: null,
    iso: null,
    exposureBias: null,
    dateTaken: null,
};

describe("formatting", () => {
    it("names the camera once", () => {
        expect(formatCamera("Canon", "Canon EOS R5")).toBe("Canon EOS R5");
        expect(formatCamera("NIKON CORPORATION", "NIKON Z 6")).toBe("NIKON Z 6");
        expect(formatCamera("Fujifilm", "X-T3")).toBe("Fujifilm X-T3");
        expect(formatCamera(null, "X-T3")).toBe("X-T3");
        expect(formatCamera("Leica", null)).toBe("Leica");
        expect(formatCamera(null, null)).toBeNull();
    });

    it("writes short exposures as a fraction and long ones in seconds", () => {
        expect(formatShutter([1, 250])).toBe("1/250 s");
        expect(formatShutter([10, 2500])).toBe("1/250 s");
        expect(formatShutter([1, 8])).toBe("1/8 s");
        expect(formatShutter([1, 2])).toBe("0.5 s");
        expect(formatShutter([30, 1])).toBe("30 s");
        expect(formatShutter([25, 10])).toBe("2.5 s");
    });

    it("writes the aperture and focal length the way a lens is marked", () => {
        expect(formatAperture(2.8)).toBe("f/2.8");
        expect(formatAperture(2)).toBe("f/2");
        expect(formatAperture(16)).toBe("f/16");
        expect(formatFocalLength(23)).toBe("23 mm");
        expect(formatFocalLength(4.25)).toBe("4.3 mm");
        expect(formatFocalLength(56.4)).toBe("56 mm");
    });

    it("signs exposure compensation", () => {
        expect(formatExposureBias(-2 / 3)).toBe("−0.7 EV");
        expect(formatExposureBias(1 / 3)).toBe("+0.3 EV");
        expect(formatExposureBias(1)).toBe("+1 EV");
        expect(formatExposureBias(0)).toBe("0 EV");
        expect(formatExposureBias(0.02)).toBe("0 EV");
    });

    it("writes the date taken, and leaves text that is not one alone", () => {
        expect(formatDateTaken("2024:03:12 14:03:22")).toBe("12 Mar 2024, 14:03");
        expect(formatDateTaken("1998:11:05")).toBe("5 Nov 1998");
        expect(formatDateTaken("0000:00:00 00:00:00")).toBe("0000:00:00 00:00:00");
        expect(formatDateTaken("yesterday")).toBe("yesterday");
    });
});

describe("the info panel's rows", () => {
    it("lists everything the file says, in order", () => {
        const rows = metadataRows(
            {
                fileName: "DSCF4127.RAF",
                make: "Fujifilm",
                model: "X-T3",
                lens: "FUJIFILM XF23mmF2 R WR",
                focalLength: 23,
                aperture: 2.8,
                exposureTime: [1, 250],
                iso: 160,
                exposureBias: -2 / 3,
                dateTaken: "2024:03:12 14:03:22",
            },
            6240,
            4160,
        );
        expect(rows).toEqual([
            { label: "File", value: "DSCF4127.RAF" },
            { label: "Dimensions", value: "6240 × 4160" },
            { label: "Camera", value: "Fujifilm X-T3" },
            { label: "Lens", value: "FUJIFILM XF23mmF2 R WR" },
            { label: "Focal length", value: "23 mm" },
            { label: "Aperture", value: "f/2.8" },
            { label: "Shutter", value: "1/250 s" },
            { label: "ISO", value: "160" },
            { label: "Exposure", value: "−0.7 EV" },
            { label: "Taken", value: "12 Mar 2024, 14:03" },
        ]);
    });

    it("leaves out what the file does not say", () => {
        expect(metadataRows(NOTHING, 4000, 3000)).toEqual([
            { label: "File", value: "scan.png" },
            { label: "Dimensions", value: "4000 × 3000" },
        ]);
        expect(hasShootingInfo(NOTHING)).toBe(false);
        expect(hasShootingInfo({ ...NOTHING, iso: 400 })).toBe(true);
    });
});
