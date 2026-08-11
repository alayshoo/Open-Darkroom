// $lib/config/keyboardShortcuts.ts

interface ShortcutActions {
    undo: () => void;
    redo: () => void;
    movePage: (right?: boolean) => void;
    changeMode: () => void;
    openImage: () => void;
    toggleFrameStats: () => void;
}

export function createKeydownHandler({ undo, redo, movePage, changeMode, openImage, toggleFrameStats }: ShortcutActions) {
    return (e: KeyboardEvent) => {
        let handled = false;
    
        if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
            undo();
            handled = true;
        } else if (e.ctrlKey && e.shiftKey && e.key === "z") {
            redo();
            handled = true;
        } else if (e.key === ".") {
            movePage(true);
            handled = true;
        } else if (e.key === ",") {
            movePage(false);
            handled = true;
        } else if (e.key === "m") {
            changeMode();
            handled = true;
        } else if (e.ctrlKey && e.key === "o") {
            openImage();
            handled = true
        } else if (e.key === "F1") {
            toggleFrameStats();
            handled = true;
        }
    
        if (handled) {
            e.preventDefault();
            (document.activeElement as HTMLElement)?.blur();
        }
    };
}