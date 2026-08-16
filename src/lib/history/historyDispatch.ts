import type { Action } from "$lib/types/historyActions";
import type { Sliders } from "$lib/types/imgParameters";

export type StateAccessors = {
    getSlider: (key: string) => number | boolean;
    setSlider: (key: string, value: number | boolean) => void;
    setSliders: (values: Partial<Sliders>) => void;
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
            break;
    }
}