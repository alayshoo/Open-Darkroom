// $lib/config/keyboardShortcuts.ts

interface ShortcutActions {
    undo: () => void;
    redo: () => void;
    movePage: (right?: boolean) => void;
    changeMode: () => void;
    openImage: () => void;
    copyAdjustments: () => void;
    pasteAdjustments: () => void;
    resetAdjustments: () => void;
    toggleDebugStats: () => void;
    toggleHistogram: () => void;
    toggleSettings: () => void;
    closeSettings: () => void;
    isSettingsOpen: () => boolean;
}

// Ctrl+C / Ctrl+V mean the ordinary thing while a field has focus, so the
// adjustments bindings stand aside for it.
function isTextField(target: EventTarget | null) {
    return (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
    );
}

export function createKeydownHandler({ undo, redo, movePage, changeMode, openImage, copyAdjustments, pasteAdjustments, resetAdjustments, toggleDebugStats, toggleHistogram, toggleSettings, closeSettings, isSettingsOpen }: ShortcutActions) {
    return (e: KeyboardEvent) => {
        let handled = false;

        // Shift changes e.key's case, so letter shortcuts compare on a
        // normalised key.
        const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;

        // The settings modal takes the keyboard while it is up: only the
        // bindings that dismiss it get through to the editing shortcuts below.
        if (isSettingsOpen()) {
            if (e.key === "Escape" || (e.ctrlKey && key === ",")) {
                closeSettings();
                handled = true;
            }
        } else if (e.ctrlKey && key === ",") {
            toggleSettings();
            handled = true;
        } else if (e.ctrlKey && key === "z" && !e.shiftKey) {
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
        } else if (e.ctrlKey && key === "c" && !e.shiftKey && !isTextField(e.target)) {
            copyAdjustments();
            handled = true;
        } else if (e.ctrlKey && key === "v" && !e.shiftKey && !isTextField(e.target)) {
            pasteAdjustments();
            handled = true;
        } else if (e.ctrlKey && e.altKey && key === "r") {
            resetAdjustments();
            handled = true;
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