import type { Action } from "$lib/types/historyActions";
import type { Sliders } from "$lib/types/imgParameters";
import type { Curves } from "$lib/types/curves";

export type StateAccessors = {
    getSlider: (key: string) => number | boolean;
    setSlider: (key: string, value: number | boolean) => void;
    setSliders: (values: Partial<Sliders>) => void;
    setCurves: (curves: Curves) => void;
    // future:
    // getMask: (id: string) => MaskData;
    // setMask: (id: string, data: MaskData) => void;
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
        // case 'mask-draw': ...
        // case 'crop': ...
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
    }
}