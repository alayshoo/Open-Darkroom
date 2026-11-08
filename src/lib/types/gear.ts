// $lib/types/gear.ts
//
// Cameras, lenses, scanners and home lab chemistry as the catalog keeps them.
// Must match GearFields and RollGear in catalog/gear.rs.

import type { StockProcess } from "./filmStock";

export type GearKind = "camera" | "lens" | "scanner" | "chemistry";

export interface GearFields {
    kind: GearKind;
    maker: string;
    name: string;
    /** Cameras and lenses: a roll on a camera is offered only the lenses that fit it. */
    mount: string;
    /** Cameras. */
    formats: string[];
    /** Chemistry. */
    process: StockProcess | "";
}

export interface Gear extends GearFields {
    id: number;
}

/** The gear a roll was shot, developed and scanned with, by id. */
export interface RollGear {
    camera: number | null;
    lenses: number[];
    scanner: number | null;
    chemistry: number | null;
}

export const NO_ROLL_GEAR: RollGear = { camera: null, lenses: [], scanner: null, chemistry: null };

export const GEAR_KINDS: { kind: GearKind; title: string; one: string; placeholder: string }[] = [
    { kind: "camera", title: "Cameras", one: "camera", placeholder: "FM2" },
    { kind: "lens", title: "Lenses", one: "lens", placeholder: "Nikkor 50mm f/1.8 AI-s" },
    { kind: "scanner", title: "Scanners", one: "scanner", placeholder: "Flextight X1" },
    { kind: "chemistry", title: "Chemistry", one: "chemistry", placeholder: "Rodinal" },
];

/** How a roll names it: the maker, then the model. Must match GearFields::label. */
export function gearLabel(gear: Pick<GearFields, "maker" | "name">): string {
    return gear.maker ? `${gear.maker} ${gear.name}` : gear.name;
}

/** Whether it matches what is typed into a picker, by maker and name. */
export function matchesGear(gear: GearFields, query: string): boolean {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const label = gearLabel(gear).toLowerCase();
    return words.every((w) => label.includes(w));
}

export function newGear(kind: GearKind, name = ""): GearFields {
    return { kind, maker: "", name: name.trim(), mount: "", formats: [], process: "" };
}

/** One line under its name: what sets it apart from the others of its kind. */
export function gearSummary(gear: GearFields): string {
    const parts =
        gear.kind === "chemistry"
            ? [gear.process]
            : [gear.mount && `${gear.mount} mount`, gear.formats.join(", ")];
    return parts.filter(Boolean).join(" · ");
}
