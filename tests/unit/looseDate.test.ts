// tests/unit/looseDate.test.ts

import { describe, expect, it } from "vitest";
import { outsideRollDates, readLooseDate } from "$lib/types/looseDate";

const day = (y: number, m: number, d: number) => Date.UTC(y, m - 1, d) / 86_400_000;

describe("reading a typed date", () => {
    it("reads a year, a month or a day, in words or numbers", () => {
        expect(readLooseDate("2024")).toEqual({ first: day(2024, 1, 1), last: day(2024, 12, 31) });
        expect(readLooseDate("March 2024")).toEqual({ first: day(2024, 3, 1), last: day(2024, 3, 31) });
        expect(readLooseDate("feb 2024")).toEqual({ first: day(2024, 2, 1), last: day(2024, 2, 29) });
        expect(readLooseDate("2024-03")).toEqual(readLooseDate("March 2024"));
        for (const text of ["12 March 2024", "March 12, 2024", "12th Mar 2024", "2024-03-12"]) {
            expect(readLooseDate(text)).toEqual({ first: day(2024, 3, 12), last: day(2024, 3, 12) });
        }
    });

    it("reads nothing it would have to guess at", () => {
        for (const text of ["", "spring", "12/03/2024", "31 April 2024", "Marching 2024", "March"]) {
            expect(readLooseDate(text)).toBeNull();
        }
    });
});

describe("a frame's date against its roll's", () => {
    it("is flagged only when it is clearly outside", () => {
        expect(outsideRollDates("12 March 2024", "March 2024", "")).toBe(false);
        expect(outsideRollDates("2 April 2024", "March 2024", undefined)).toBe(true);
        expect(outsideRollDates("2 April 2024", "March 2024", "May 2024")).toBe(false);
        expect(outsideRollDates("28 February 2024", "1 March 2024", "May 2024")).toBe(true);
        expect(outsideRollDates("June 2024", "March 2024", "May 2024")).toBe(true);
    });

    it("is not flagged when either date cannot be read", () => {
        expect(outsideRollDates("last summer", "March 2024", "")).toBe(false);
        expect(outsideRollDates("12 March 2023", "spring", "")).toBe(false);
        expect(outsideRollDates("12 March 2023", "", "")).toBe(false);
        expect(outsideRollDates("12 March 2023", "March 2024", "sometime")).toBe(false);
    });
});
