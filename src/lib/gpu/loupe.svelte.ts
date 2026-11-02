// $lib/gpu/loupe.svelte.ts
//
// The unsharp mask's 1:1 loupe, shared between the sidebar that shows it and
// the preview that renders it and places it.

import { untrack } from "svelte";
import type { GPUCanvasLink } from "$lib/types/gpuTypes";
import type { Point } from "$lib/utils/viewport";

export interface LoupeTarget {
    link: GPUCanvasLink;
    // Device pixels across; the canvas is square.
    size: number;
}

class Loupe {
    /** Whether the loupe's square is being placed on the preview. */
    picking = $state(false);

    /** The image point the loupe is centred on, or null for the crop's centre. */
    position = $state.raw<Point | null>(null);

    /** No image, the full resolution still streaming in, or ready to show at 1:1. */
    status = $state<"none" | "loading" | "ready">("none");

    /** Device pixels across the last loupe measured, for sizing its square. */
    size = $state(0);

    // Every loupe currently on screen. Both modes have one, and both are
    // mounted while the controls slide between them; the latest one wins.
    #shown = $state.raw<{ id: number; target: LoupeTarget }[]>([]);
    #nextId = 0;

    get target(): LoupeTarget | null {
        return this.#shown.at(-1)?.target ?? null;
    }

    register(): number {
        return this.#nextId++;
    }

    /** Where loupe `id` draws, or null once it is off screen. */
    show(id: number, target: LoupeTarget | null) {
        const rest = untrack(() => this.#shown).filter((s) => s.id !== id);
        this.#shown = target ? [...rest, { id, target }] : rest;
        if (target) this.size = target.size;
    }

    toggle() {
        this.picking = !this.picking;
    }
}

export const loupe = new Loupe();
