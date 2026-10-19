// $lib/config/shortcutRegistry.ts

/**
 * Every rebindable action, and the keys it answers to out of the box.
 *
 * A binding is one string — "ctrl+shift+z", "F1", "," — so a registry entry and
 * a line in the settings file are the same shape, and an action can hold as
 * many of them as the user cares to add. Adding an action means an entry below
 * plus a handler where the page wires them up; nothing else reads the list.
 *
 * Fixed navigation stays out of here: the mouse side buttons page the slider
 * strip on their own and are not the user's to change.
 */

export interface Binding {
    /** Single characters lowercased; anything longer as KeyboardEvent.key spells it. */
    key: string;
    ctrl: boolean;
    shift: boolean;
    alt: boolean;
    meta: boolean;
}

export interface ShortcutAction {
    /** Stable id; what the stored overrides and the handler map are keyed by. */
    id: string;
    label: string;
    description?: string;
    /** Which group in `shortcutGroups` the row belongs to. */
    group: string;
    defaults: readonly string[];
}

export interface ShortcutGroup {
    id: string;
    label: string;
    note?: string;
}

/** Keys that only ever qualify another key, so never a binding on their own. */
const MODIFIER_KEYS = new Set([
    "Control",
    "Shift",
    "Alt",
    "Meta",
    "AltGraph",
    "CapsLock",
    "OS",
]);

/** Keys whose own name reads badly on a chip. */
const KEY_LABELS: Record<string, string> = {
    " ": "Space",
    ArrowUp: "↑",
    ArrowDown: "↓",
    ArrowLeft: "←",
    ArrowRight: "→",
    Escape: "Esc",
    Backspace: "⌫",
    Delete: "Del",
};

function normaliseKey(key: string) {
    return key.length === 1 ? key.toLowerCase() : key;
}

export function serialize(b: Binding): string {
    const parts: string[] = [];
    if (b.ctrl) parts.push("ctrl");
    if (b.alt) parts.push("alt");
    if (b.shift) parts.push("shift");
    if (b.meta) parts.push("meta");
    parts.push(b.key);
    return parts.join("+");
}

/**
 * The inverse, lenient enough to survive a hand-edited file. Modifier names are
 * taken off the front until one is not recognised; whatever is left is the key,
 * rejoined — which is what keeps "ctrl++" meaning Ctrl and the plus key.
 */
export function parse(serialized: string): Binding | null {
    const parts = serialized.split("+");
    const b: Binding = { key: "", ctrl: false, shift: false, alt: false, meta: false };

    let i = 0;
    for (; i < parts.length - 1; i++) {
        const token = parts[i].toLowerCase();
        if (token === "ctrl") b.ctrl = true;
        else if (token === "shift") b.shift = true;
        else if (token === "alt") b.alt = true;
        else if (token === "meta") b.meta = true;
        else break;
    }

    const key = parts.slice(i).join("+");
    if (!key) return null;
    b.key = normaliseKey(key);
    return b;
}

/** What a key press would bind to, or null while only modifiers are down. */
export function bindingFromEvent(e: KeyboardEvent): Binding | null {
    if (MODIFIER_KEYS.has(e.key)) return null;
    return {
        key: normaliseKey(e.key),
        ctrl: e.ctrlKey,
        shift: e.shiftKey,
        alt: e.altKey,
        meta: e.metaKey,
    };
}

/**
 * Modifiers have to agree exactly, not merely be present. That is what keeps
 * Ctrl+Z off Ctrl+Shift+Z, and a bare comma off the settings binding.
 */
export function matchesEvent(serialized: string, e: KeyboardEvent): boolean {
    const b = parse(serialized);
    if (!b) return false;
    return (
        normaliseKey(e.key) === b.key &&
        e.ctrlKey === b.ctrl &&
        e.shiftKey === b.shift &&
        e.altKey === b.alt &&
        e.metaKey === b.meta
    );
}

/** How a binding reads on a chip or in a menu: "Ctrl+Shift+Z". */
export function display(serialized: string): string {
    const b = parse(serialized);
    if (!b) return "";

    const parts: string[] = [];
    if (b.ctrl) parts.push("Ctrl");
    if (b.alt) parts.push("Alt");
    if (b.shift) parts.push("Shift");
    if (b.meta) parts.push("Meta");
    parts.push(KEY_LABELS[b.key] ?? (b.key.length === 1 ? b.key.toUpperCase() : b.key));
    return parts.join("+");
}

export const shortcutGroups: readonly ShortcutGroup[] = [
    {
        id: "navigation",
        label: "Navigation",
        note: "The mouse side buttons page the strip as well, and are fixed.",
    },
    { id: "file", label: "File" },
    { id: "edit", label: "Edit" },
    {
        id: "debug",
        label: "Debug",
        note: "Live only while the debug tools are on.",
    },
];

export const shortcutActions = [
    {
        id: "pagePrev",
        group: "navigation",
        label: "Previous tab",
        description: "",
        defaults: [",", "a"],
    },
    {
        id: "pageNext",
        group: "navigation",
        label: "Next tab",
        description: "",
        defaults: [".", "d"],
    },
    {
        id: "changeMode",
        group: "navigation",
        label: "Switch mode",
        description: "Between Dark Room mode and Light Table mode.",
        defaults: ["m", "s"],
    },
    {
        id: "openImage",
        group: "file",
        label: "Open image",
        defaults: ["ctrl+o"],
    },
    {
        id: "toggleSettings",
        group: "file",
        label: "Settings",
        description: "Opens this window, and closes it again.",
        defaults: ["ctrl+,"],
    },
    { id: "undo", group: "edit", label: "Undo", defaults: ["ctrl+z"] },
    { id: "redo", group: "edit", label: "Redo", defaults: ["ctrl+shift+z"] },
    {
        id: "copyAdjustments",
        group: "edit",
        label: "Copy adjustments",
        defaults: ["ctrl+c"],
    },
    {
        id: "pasteAdjustments",
        group: "edit",
        label: "Paste adjustments",
        defaults: ["ctrl+v"],
    },
    {
        id: "resetAdjustments",
        group: "edit",
        label: "Reset adjustments",
        defaults: ["ctrl+alt+r"],
    },
    {
        id: "toggleDebugStats",
        group: "debug",
        label: "Frame stats overlay",
        defaults: ["F1"],
    },
    {
        id: "toggleHistogram",
        group: "debug",
        label: "Live histogram",
        defaults: ["F2"],
    },
    {
        id: "toggleColorSpace",
        group: "debug",
        label: "Colour space panel",
        description: "The highlight roll-off, the chromaticity diagram and the colour volume, on the left.",
        defaults: ["F3"],
    },
] as const satisfies readonly ShortcutAction[];

export type ShortcutId = (typeof shortcutActions)[number]["id"];

/** The actions in a group, in registry order. */
export function actionsIn(group: string) {
    return shortcutActions.filter((a) => a.group === group);
}
