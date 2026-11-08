// $lib/types/filmStock.ts
//
// A film stock as the catalog keeps it. Must match StockFields in
// catalog/film_stocks.rs.

export type StockKind = "colourNegative" | "slide" | "blackAndWhite";
export type StockProcess = "C-41" | "E-6" | "B&W" | "ECN-2";

export interface StockFields {
    maker: string;
    name: string;
    boxIso: number | null;
    kind: StockKind;
    process: StockProcess;
    formats: string[];
    /** `#rrggbb`. */
    colour: string | null;
    colour2: string | null;
}

export interface FilmStock extends StockFields {
    id: number;
}

export const STOCK_KINDS: { value: StockKind; label: string }[] = [
    { value: "colourNegative", label: "Colour negative" },
    { value: "slide", label: "Slide" },
    { value: "blackAndWhite", label: "Black and white" },
];

export const STOCK_PROCESSES: StockProcess[] = ["C-41", "E-6", "B&W", "ECN-2"];

export const STOCK_FORMATS = ["35mm", "120", "Sheet", "16mm"];

/** The process a stock of this kind usually goes through. */
export function usualProcess(kind: StockKind): StockProcess {
    return kind === "slide" ? "E-6" : kind === "blackAndWhite" ? "B&W" : "C-41";
}

/** How a roll names the stock: the maker, then the stock. Must match StockFields::label. */
export function stockLabel(stock: Pick<StockFields, "maker" | "name">): string {
    return stock.maker ? `${stock.maker} ${stock.name}` : stock.name;
}

export function kindLabel(kind: StockKind): string {
    return STOCK_KINDS.find((k) => k.value === kind)?.label ?? kind;
}

/** A stock made from what was typed: a trailing number is taken as its box ISO. */
export function stockFromText(text: string): StockFields {
    const trimmed = text.trim();
    const iso = trimmed.match(/(\d{2,4})\s*$/);
    return {
        maker: "",
        name: trimmed,
        boxIso: iso ? Number(iso[1]) : null,
        kind: "colourNegative",
        process: "C-41",
        formats: [],
        colour: null,
        colour2: null,
    };
}

/** Whether a stock matches what is typed into the picker, by maker and name. */
export function matchesStock(stock: StockFields, query: string): boolean {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const label = stockLabel(stock).toLowerCase();
    return words.every((w) => label.includes(w));
}
