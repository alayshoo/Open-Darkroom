// src/lib/gpu/previewDebug.svelte.ts
//
// What the preview is holding and drawing, for the F3 panel's preview card.
// The renderer only reports while the card is mounted.

import type { Box, ChainRegion } from "$lib/utils/viewport";

export interface PreviewState {
    fullWidth: number;
    fullHeight: number;
    // The part of the image on screen, in image pixels.
    visible: Box | null;
    // The canvas pixels per image pixel the view asks for.
    scale: number;
    // The sharp region the chain holds, and the whole-image backdrop under it.
    detail: ChainRegion | null;
    backdrop: ChainRegion | null;
    // Whether the last frame showed the backdrop anywhere.
    backdropShown: boolean;
    source: { width: number; height: number; mipLevels: number };
}

export interface PreviewCounters {
    detailRenders: number;
    backdropRenders: number;
    // Frames drawn from what was held while a fresh render was put off.
    deferred: number;
    lastDetailMs: number;
}

export interface InputEvent {
    type: string;
    detail: string;
    at: number;
}

class PreviewDebug {
    /** Set by the card while it is mounted; nothing is reported otherwise. */
    enabled = false;

    state = $state.raw<PreviewState | null>(null);
    counters = $state.raw<PreviewCounters>({ detailRenders: 0, backdropRenders: 0, deferred: 0, lastDetailMs: 0 });
    // Rows of the full-resolution image on the GPU so far, out of how many.
    stream = $state.raw<{ rows: number; total: number; done: boolean } | null>(null);
    input = $state.raw<InputEvent | null>(null);

    noteInput(type: string, detail: string) {
        if (this.enabled) this.input = { type, detail, at: performance.now() };
    }
}

export const previewDebug = new PreviewDebug();
