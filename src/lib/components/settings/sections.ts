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
import LibrarySection from "./sections/LibrarySection.svelte";
import FilmStocksSection from "./sections/FilmStocksSection.svelte";
import GearSection from "./sections/GearSection.svelte";
import HomeLabSection from "./sections/HomeLabSection.svelte";

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
        id: "library",
        label: "Library",
        icon: "M4 6.5h16v11H4zM4 14l4.5-4 4 3.5 3-2.5 4.5 4",
        component: LibrarySection,
    },
    {
        id: "film-stocks",
        label: "Film stocks",
        icon: "M5 6.5h9v13H5zM7.5 6.5V4.3h4v2.2M14 9.5h4.5l1.5 1.5v5.5H14",
        component: FilmStocksSection,
    },
    {
        id: "gear",
        label: "Gear",
        icon: "M3.5 8h4l1.5-2.5h6L16.5 8h4v11h-17zM12 10.5a3.5 3.5 0 1 0 0 7a3.5 3.5 0 1 0 0-7",
        component: GearSection,
    },
    {
        id: "home-lab",
        label: "Home lab",
        icon: "M9.5 3.5h5M10.5 3.5v6L5 19a1 1 0 0 0 .9 1.5h12.2a1 1 0 0 0 .9-1.5l-5.5-9.5v-6M7.5 15h9",
        component: HomeLabSection,
    },
    {
        id: "debug",
        label: "Debug",
        icon: "M4 13h3l2.5-6 4 12 2.5-6h4",
        component: DebugSection,
    },
];
