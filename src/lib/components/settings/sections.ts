// $lib/components/settings/sections.ts

/**
 * The settings modal's categories, in sidebar order.
 *
 * Adding one means writing a section component out of SettingsGroup/SettingsRow
 * and appending an entry here — the modal reads nothing else.
 */

import type { Component } from "svelte";
import DebugSection from "./sections/DebugSection.svelte";
import ShortcutsSection from "./sections/ShortcutsSection.svelte";

export interface SettingsSection {
    /** Stable id; what `settings.section` holds. */
    id: string;
    label: string;
    /** Path data for the sidebar glyph, drawn stroked in a 24×24 box. */
    icon: string;
    component: Component;
}

export const settingsSections: SettingsSection[] = [
    {
        id: "shortcuts",
        label: "Shortcuts",
        icon: "M3 7h18v10H3zM7 11h.01M11 11h.01M15 11h.01M8.5 15h7",
        component: ShortcutsSection,
    },
    {
        id: "debug",
        label: "Debug",
        icon: "M4 13h3l2.5-6 4 12 2.5-6h4",
        component: DebugSection,
    },
];
