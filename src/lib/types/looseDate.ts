// $lib/types/looseDate.ts
//
// Dates as people type them into a roll or a frame: a year, a month, or a
// day, in words or numbers. Read only to compare them, never to rewrite them.

const MONTHS = [
    "january", "february", "march", "april", "may", "june",
    "july", "august", "september", "october", "november", "december",
];

/** The days a typed date covers, as UTC day numbers from the first to the last. */
export interface DaySpan {
    first: number;
    last: number;
}

const DAY_MS = 86_400_000;
const dayOf = (year: number, month: number, day: number) => Date.UTC(year, month, day) / DAY_MS;

function span(year: number, month: number | null, day: number | null): DaySpan | null {
    if (year < 1800 || year > 2200) return null;
    if (month === null) return { first: dayOf(year, 0, 1), last: dayOf(year, 11, 31) };
    if (month < 0 || month > 11) return null;
    if (day === null) return { first: dayOf(year, month, 1), last: dayOf(year, month + 1, 0) };
    const first = dayOf(year, month, day);
    if (new Date(first * DAY_MS).getUTCMonth() !== month) return null;
    return { first, last: first };
}

/** A month by its English name, or the first three or four letters of it. */
const monthNamed = (word: string) => {
    const i = MONTHS.findIndex((name) => name === word || (word.length >= 3 && word.length <= 4 && name.startsWith(word)));
    return i >= 0 ? i : null;
};

/**
 * Reads "2024", "2024-03", "2024-03-12", "March 2024", "12 March 2024" and
 * "March 12, 2024". Null for anything else, including 12/03/2024, which could
 * be either day or month first.
 */
export function readLooseDate(text: string | undefined): DaySpan | null {
    const t = (text ?? "").trim().toLowerCase().replace(/(\d)(st|nd|rd|th)\b/g, "$1");
    if (!t) return null;
    let m = t.match(/^(\d{4})(?:-(\d{1,2})(?:-(\d{1,2}))?)?$/);
    if (m) return span(+m[1], m[2] ? +m[2] - 1 : null, m[3] ? +m[3] : null);
    const words = t.split(/[\s,.]+/).filter(Boolean);
    if (words.length < 2 || words.length > 3) return null;
    const year = +words[words.length - 1];
    if (!/^\d{4}$/.test(words[words.length - 1])) return null;
    if (words.length === 2) {
        const month = monthNamed(words[0]);
        return month === null ? null : span(year, month, null);
    }
    const [a, b] = words;
    if (/^\d{1,2}$/.test(a) && monthNamed(b) !== null) return span(year, monthNamed(b), +a);
    if (/^\d{1,2}$/.test(b) && monthNamed(a) !== null) return span(year, monthNamed(a), +b);
    return null;
}

/**
 * Whether a frame's date falls outside the dates its roll was shot over. False
 * when either cannot be read, so only a date that is clearly outside is flagged.
 */
export function outsideRollDates(date: string | undefined, from: string | undefined, until: string | undefined): boolean {
    const frame = readLooseDate(date);
    const start = readLooseDate(from);
    if (!frame || !start) return false;
    const end = until?.trim() ? readLooseDate(until) : start;
    if (!end) return false;
    return frame.last < start.first || frame.first > end.last;
}
