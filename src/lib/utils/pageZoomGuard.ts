/* $lib/utils/pageZoomGuard.ts */

// On Windows the webview's own zoom is left on, since that is what lets a
// trackpad pinch reach the page as a Ctrl + wheel event. Cancelling the events
// that would zoom the whole interface keeps the pinch for the preview alone.

const ZOOM_KEYS = new Set(["=", "+", "-", "_", "0"]);

function onWheel(e: WheelEvent) {
    if (e.ctrlKey) e.preventDefault();
}

function onKeydown(e: KeyboardEvent) {
    if (e.ctrlKey && ZOOM_KEYS.has(e.key)) e.preventDefault();
}

/** Holds off page zoom until the returned function is called. */
export function holdPageZoom(): () => void {
    window.addEventListener("wheel", onWheel, { passive: false });
    window.addEventListener("keydown", onKeydown);
    return () => {
        window.removeEventListener("wheel", onWheel);
        window.removeEventListener("keydown", onKeydown);
    };
}
