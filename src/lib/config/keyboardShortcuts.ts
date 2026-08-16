// $lib/config/keyboardShortcuts.ts

interface ShortcutActions {
    undo: () => void;
    redo: () => void;
    movePage: (right?: boolean) => void;
    changeMode: () => void;
    openImage: () => void;
    toggleDebugStats: () => void;
    toggleHistogram: () => void;
}

export function createKeydownHandler({ undo, redo, movePage, changeMode, openImage, toggleDebugStats, toggleHistogram }: ShortcutActions) {
    return (e: KeyboardEvent) => {
        let handled = false;

        // Shift changes e.key's case, so letter shortcuts compare on a
        // normalised key.
        const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

        if (e.ctrlKey && key === "z" && !e.shiftKey) {
            undo();
            handled = true;
        } else if (e.ctrlKey && e.shiftKey && key === "z") {
            redo();
            handled = true;
        } else if (e.key === ".") {
            movePage(true);
            handled = true;
        } else if (e.key === ",") {
            movePage(false);
            handled = true;
        } else if (key === "m") {
            changeMode();
            handled = true;
        } else if (e.ctrlKey && key === "o") {
            openImage();
            handled = true
        } else if (e.key === "F1") {
            toggleDebugStats();
            handled = true;
        } else if (e.key === "F2") {
            toggleHistogram();
            handled = true;
        }
    
        if (handled) {
            e.preventDefault();
            (document.activeElement as HTMLElement)?.blur();
        }
    };
}