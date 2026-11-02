import type { Action } from "$lib/types/historyActions";
import { type Sliders, HSL_KEYS, isDarkroomKey, sharedModeKeys } from "$lib/types/imgParameters";

export type ControlMode = "darkroom" | "lighter-room";

const UNSHARP_MASK: (keyof Sliders)[] = ["usmAmount", "usmRadius", "usmLumaThreshold", "usmDetailThreshold"];

// The pages each slider appears on, in pager order. Invert sits above the
// darkroom pager, on no page of its own.
export const LIGHTER_ROOM_PAGES: readonly (readonly (keyof Sliders)[])[] = [
    ["clarity", "texture", ...UNSHARP_MASK],
    ["wbTemp", "wbTint", "exposure", "contrast", "brightness", "vibrance", "saturation",
        "highlights", "shadows", "whites", "blacks"],
    ["hue", "vibrance", "saturation", ...HSL_KEYS],
];
export const DARKROOM_PAGES: readonly (readonly (keyof Sliders)[])[] = [
    UNSHARP_MASK,
    ["redBlackPoint", "redGamma", "redWhitePoint", "greenBlackPoint", "greenGamma", "greenWhitePoint",
        "blueBlackPoint", "blueGamma", "blueWhitePoint", "masterBlackPoint", "masterGamma",
        "masterWhitePoint", "rgbOutputBlack", "rgbOutputWhite"],
];
export const CURVES_PAGE = 2;

function actionKeys(a: Action): (keyof Sliders)[] {
    if (a.type === "slider") return [a.key];
    if (a.type === "sliders") return Object.keys(a.newValues) as (keyof Sliders)[];
    return [];
}

const hasCurves = (a: Action) => a.type === "curves" || (a.type === "sliders" && !!a.newCurves);

/** The mode an action's controls belong to, or null when both modes show them. */
export function actionMode(a: Action): ControlMode | null {
    if (hasCurves(a)) return "lighter-room";
    const own = actionKeys(a).filter((k) => !sharedModeKeys.has(k));
    if (own.length === 0) return null;
    return own.every(isDarkroomKey) ? "darkroom" : "lighter-room";
}

/** The page of `mode` holding an action's controls, or null to stay on
 *  `currentPage` because it already shows one of them or none has them. */
export function actionPage(a: Action, mode: ControlMode, currentPage: number): number | null {
    const pages = mode === "darkroom" ? DARKROOM_PAGES : LIGHTER_ROOM_PAGES;
    const keys = actionKeys(a);
    const curves = mode === "lighter-room" && hasCurves(a);
    const onPage = (i: number) =>
        (curves && i === CURVES_PAGE) || keys.some((k) => pages[i].includes(k));

    if (onPage(currentPage)) return null;
    const page = pages.findIndex((_, i) => onPage(i));
    return page === -1 ? null : page;
}
