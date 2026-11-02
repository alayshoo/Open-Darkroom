import type { Action } from "$lib/types/historyActions";
import { type Sliders, HSL_KEYS, isDarkroomKey } from "$lib/types/imgParameters";

export type ControlMode = "darkroom" | "lighter-room";

// The lighter-room pages each slider appears on, in pager order.
export const LIGHTER_ROOM_PAGES: readonly (readonly (keyof Sliders)[])[] = [
    ["clarity", "texture", "usmAmount", "usmRadius", "usmLumaThreshold", "usmDetailThreshold"],
    ["wbTemp", "wbTint", "exposure", "contrast", "brightness", "vibrance", "saturation",
        "highlights", "shadows", "whites", "blacks"],
    ["hue", "vibrance", "saturation", ...HSL_KEYS],
];
export const CURVES_PAGE = 2;

/** Which mode's controls an action belongs to, or null for one shared by both. */
export function actionMode(a: Action): ControlMode | null {
    switch (a.type) {
        case "slider":
            return isDarkroomKey(a.key) ? "darkroom" : "lighter-room";
        case "sliders": {
            if (a.newCurves) return "lighter-room";
            const keys = Object.keys(a.newValues) as (keyof Sliders)[];
            if (keys.length === 0) return null;
            return keys.every(isDarkroomKey) ? "darkroom" : "lighter-room";
        }
        case "curves":
            return "lighter-room";
        case "crop":
            return null;
    }
}

/** The lighter-room page holding an action's controls, or null to stay on
 *  `currentPage` because it already shows one of them. */
export function actionPage(a: Action, currentPage: number): number | null {
    const keys: (keyof Sliders)[] =
        a.type === "slider" ? [a.key]
        : a.type === "sliders" ? (Object.keys(a.newValues) as (keyof Sliders)[])
        : [];
    const hasCurves = a.type === "curves" || (a.type === "sliders" && !!a.newCurves);
    const onPage = (i: number) =>
        (hasCurves && i === CURVES_PAGE) ||
        keys.some((k) => LIGHTER_ROOM_PAGES[i].includes(k));

    if (onPage(currentPage)) return null;
    const page = LIGHTER_ROOM_PAGES.findIndex((_, i) => onPage(i));
    return page === -1 ? null : page;
}
