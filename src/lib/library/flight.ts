// $lib/library/flight.ts
//
// The image flying between its grid cell and the editor, done by hand with the
// FLIP technique: the flyer is laid out where it lands and animated back from
// where it took off, by transform alone.

export interface Rect {
    left: number;
    top: number;
    width: number;
    height: number;
}

export const FLIGHT_MS = 420;
const EASING = "cubic-bezier(0.2, 0, 0, 1)";

/** The largest rect of `aspect` (width / height) centred in `box`. */
export function fitRect(aspect: number, box: Rect): Rect {
    const width = Math.min(box.width, box.height * aspect);
    const height = width / aspect;
    return { left: box.left + (box.width - width) / 2, top: box.top + (box.height - height) / 2, width, height };
}

export function rectOf(el: Element | null): Rect | null {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 ? { left: r.left, top: r.top, width: r.width, height: r.height } : null;
}

/** The part of an `<img>` the picture fills, inside the box it is laid out in. */
export function imageRect(img: HTMLImageElement | null): Rect | null {
    const box = rectOf(img);
    if (!img || !box || !img.naturalWidth) return box;
    return fitRect(img.naturalWidth / img.naturalHeight, box);
}

export function nextFrame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

const reducedMotion = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

/** Flies `src` from one rect to another and resolves once it has landed. The
 *  flyer is left in place for the caller to hand over from, then remove. */
export async function fly(src: string, from: Rect, to: Rect): Promise<HTMLImageElement> {
    const el = document.createElement("img");
    el.src = src;
    el.className = "library-flyer";
    el.alt = "";
    Object.assign(el.style, {
        position: "fixed",
        left: `${to.left}px`,
        top: `${to.top}px`,
        width: `${to.width}px`,
        height: `${to.height}px`,
        transformOrigin: "0 0",
        zIndex: "40",
        pointerEvents: "none",
        objectFit: "fill",
    });
    document.body.appendChild(el);
    try {
        await el.decode();
    } catch {
        // A thumbnail that fails to decode still flies as an empty box.
    }

    const dx = from.left - to.left;
    const dy = from.top - to.top;
    const sx = from.width / to.width;
    const sy = from.height / to.height;
    const animation = el.animate(
        [{ transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})` }, { transform: "none" }],
        { duration: reducedMotion() ? 0 : FLIGHT_MS, easing: EASING },
    );
    try {
        await animation.finished;
    } catch {
        // Cancelled: it lands where it was going all the same.
    }
    return el;
}

/** Moves a landed flyer onto a rect it was not aimed at, such as an image shown smaller than its slot. */
export async function settle(el: HTMLImageElement, to: Rect, ms = 220): Promise<void> {
    const from = rectOf(el);
    if (!from || (Math.abs(from.left - to.left) < 1 && Math.abs(from.width - to.width) < 1)) return;
    Object.assign(el.style, { left: `${to.left}px`, top: `${to.top}px`, width: `${to.width}px`, height: `${to.height}px` });
    const t = `translate(${from.left - to.left}px, ${from.top - to.top}px) scale(${from.width / to.width}, ${from.height / to.height})`;
    try {
        await el.animate([{ transform: t }, { transform: "none" }], { duration: reducedMotion() ? 0 : ms, easing: EASING })
            .finished;
    } catch {
        // Settled where it was going either way.
    }
}

/** Fades a landed flyer out over whatever has taken its place, then removes it. */
export async function land(el: HTMLImageElement, fadeMs = 160): Promise<void> {
    try {
        await el.animate([{ opacity: 1 }, { opacity: 0 }], { duration: reducedMotion() ? 0 : fadeMs }).finished;
    } catch {
        // Removed either way.
    }
    el.remove();
}
