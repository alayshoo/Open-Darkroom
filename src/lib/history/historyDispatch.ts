import type { Action } from "$lib/types/historyActions";
import type { Sliders } from "$lib/types/imgParameters";
import type { Curves } from "$lib/types/curves";
import type { Crop } from "$lib/types/crop";
import type { Retouch } from "$lib/types/retouch";

export type StateAccessors = {
    getSlider: (key: string) => number | boolean;
    setSlider: (key: string, value: number | boolean) => void;
    setSliders: (values: Partial<Sliders>) => void;
    setCurves: (curves: Curves) => void;
    setCrop: (crop: Crop) => void;
    getRetouch: () => Retouch[];
    setRetouch: (strokes: Retouch[]) => void;
};

export function applyAction(action: Action, state: StateAccessors) {
    switch (action.type) {
        case "slider":
            state.setSlider(action.key, action.newValue);
            break;
        case "sliders":
            state.setSliders(action.newValues);
            if (action.newCurves) state.setCurves(action.newCurves);
            break;
        case "curves":
            state.setCurves(action.newCurves);
            break;
        case "crop":
            state.setCrop(action.newValue);
            break;
        case "retouchAdd":
            state.setRetouch([...state.getRetouch(), action.stroke]);
            break;
        case "retouchClear":
            state.setRetouch([]);
            break;
    }
}

export function undoAction(action: Action, state: StateAccessors) {
    switch (action.type) {
        case "slider":
            state.setSlider(action.key, action.oldValue);
            break;
        case "sliders":
            state.setSliders(action.oldValues);
            if (action.oldCurves) state.setCurves(action.oldCurves);
            break;
        case "curves":
            state.setCurves(action.oldCurves);
            break;
        case "crop":
            state.setCrop(action.oldValue);
            break;
        case "retouchAdd":
            state.setRetouch(state.getRetouch().slice(0, -1));
            break;
        case "retouchClear":
            state.setRetouch(action.strokes);
            break;
    }
}