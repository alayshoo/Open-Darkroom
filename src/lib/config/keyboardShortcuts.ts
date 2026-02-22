// $lib/config/keyboardShortcuts.ts

interface ShortcutActions {
    undo: () => void;
    redo: () => void;
}

export function createKeydownHandler({ undo, redo }: ShortcutActions) {
    return (e: KeyboardEvent) => {
        if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
            e.preventDefault();
            undo();
        } else if (
            (e.ctrlKey && e.key === "Z") ||
            (e.ctrlKey && e.shiftKey && e.key === "z")
        ) {
            e.preventDefault();
            redo();
        }
    };
}