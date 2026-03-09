import type { Action } from "$lib/types/historyActions";

export type StateAccessors = {
    getSlider: (key: string) => number | boolean;
    setSlider: (key: string, value: number | boolean) => void;
    // future:
    // getMask: (id: string) => MaskData;
    // setMask: (id: string, data: MaskData) => void;
};

export function applyAction(action: Action, state: StateAccessors) {
    switch (action.type) {
        case "slider":
            state.setSlider(action.key, action.newValue);
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
    }
}