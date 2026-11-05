// $lib/retouch/retouchTool.svelte.ts
//
// The heal and clone brushes, shared between the toolbar that picks them, the
// panel that sets them and the preview that paints with them. Kept from one
// image to the next, except for the source, which belongs to the image.

import type { RetouchMode } from "$lib/types/retouch";
import type { Point } from "$lib/utils/viewport";

/** Brush radius limits, in full-resolution image pixels. */
export const MIN_BRUSH_RADIUS = 1;
export const MAX_BRUSH_RADIUS = 400;

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

class RetouchTool {
    /** The brush in hand, or null for none. */
    mode = $state<RetouchMode | null>(null);

    /** In full-resolution image pixels. */
    radius = $state(12);

    /** How much of the radius the edge fades over, in percent. */
    feather = $state(50);

    /** In percent. */
    opacity = $state(100);

    /** An image point picked as the source, waiting for the stroke it is measured from. */
    pickedSource = $state.raw<Point | null>(null);

    // Image pixels from a stroke to its source, kept for every stroke after a
    // picked source is used. Null lets each stroke find its own.
    offset = $state.raw<Point | null>(null);

    toggle(mode: RetouchMode) {
        this.mode = this.mode === mode ? null : mode;
    }

    setRadius(radius: number) {
        this.radius = clamp(radius, MIN_BRUSH_RADIUS, MAX_BRUSH_RADIUS);
    }

    setFeather(feather: number) {
        this.feather = clamp(feather, 0, 100);
    }

    pickSource(point: Point) {
        this.pickedSource = point;
        this.offset = null;
    }

    /** The offset a stroke starting at `start` takes, or null to search for one. */
    offsetFor(start: Point): Point | null {
        if (this.pickedSource) {
            this.offset = [this.pickedSource[0] - start[0], this.pickedSource[1] - start[1]];
            this.pickedSource = null;
        }
        return this.offset;
    }

    resetSource() {
        this.pickedSource = null;
        this.offset = null;
    }

    get hasSource(): boolean {
        return this.pickedSource !== null || this.offset !== null;
    }
}

export const retouchTool = new RetouchTool();
