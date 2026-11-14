
// lib/gpu/gpu.ts

import { type GPUSession } from "$lib/types/gpuTypes";
import { createLogger } from "$lib/utils/log";
import type { OutputSpaceId } from "./outputSpace";

const log = createLogger("gpu");

export async function initializeGPU(): Promise<GPUSession> {
    
    // Check if WebGPU is available
    if (!navigator.gpu) {
        throw new Error("WebGPU not supported in this device");
    }

    // Request an adapter — this is the physical GPU.
    // Returns null if no suitable GPU is found.
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) {
        throw new Error("No GPU adapter found");
    }

    // Timestamp queries drive the per-pass timings the frame stats overlay
    // shows. Optional: not every adapter exposes them, and nothing but the
    // overlay depends on them.
    const canTimestamp = adapter.features.has("timestamp-query");

    // Request a device — logical handle
    // All GPU objects (buffers, textures, pipelines) belong to a device.
    // The largest texture the adapter allows, rather than the default 8192, so
    // most full-resolution images fit in one texture.
    const device = await adapter.requestDevice({
        requiredFeatures: canTimestamp ? ["timestamp-query"] : [],
        requiredLimits: { maxTextureDimension2D: adapter.limits.maxTextureDimension2D },
    });

    // Format is the pixel format the display expects.
    const format = navigator.gpu.getPreferredCanvasFormat();
    const displaySpace = detectDisplaySpace();

    log.info(`Adapter: ${describeAdapter(adapter.info)}`);
    log.info(
        `Device: canvas ${format}, display ${displaySpace}, ` +
            `max texture ${device.limits.maxTextureDimension2D}px, ` +
            `max buffer ${Math.round(device.limits.maxBufferSize / 2 ** 20)} MiB, ` +
            `timestamps ${canTimestamp ? "on" : "off"}`,
    );
    watchDevice(device);

    return {
        device,
        format,
        displaySpace,
        canTimestamp,
        adapterInfo: adapter.info ?? null,
        adapterFeatures: [...adapter.features].sort(),
    };
}

/** One line naming the adapter, for the log and for bug reports. */
export function describeAdapter(info: GPUAdapterInfo | null | undefined): string {
    if (!info) return "no adapter info";
    const parts = [info.vendor, info.architecture, info.device, info.description].filter(Boolean);
    return (parts.join(" / ") || "details withheld") + (info.isFallbackAdapter ? " (fallback)" : "");
}

/** Copies of one uncaptured error logged before the rest are only counted. */
const REPEATS_LOGGED = 3;
const DISTINCT_ERRORS_TRACKED = 100;

/**
 * Validation and out-of-memory errors no error scope caught would otherwise
 * vanish outside devtools, and a lost device just leaves the canvas frozen.
 */
function watchDevice(device: GPUDevice) {
    const seen = new Map<string, number>();
    let saturated = false;
    device.addEventListener("uncapturederror", (event) => {
        const { error } = event as GPUUncapturedErrorEvent;
        const key = `${error.constructor.name}: ${error.message}`;
        if (!seen.has(key) && seen.size >= DISTINCT_ERRORS_TRACKED) {
            if (!saturated) log.error("Too many distinct uncaptured errors, the rest are not logged");
            saturated = true;
            return;
        }
        const count = (seen.get(key) ?? 0) + 1;
        seen.set(key, count);
        if (count > REPEATS_LOGGED) return;
        const more = count === REPEATS_LOGGED ? " (further repeats are not logged)" : "";
        log.error(`Uncaptured ${key}${more}`);
    });

    device.lost.then((info) => {
        if (info.reason === "destroyed") log.info("Device destroyed");
        else log.error(`Device lost (${info.reason}): ${info.message}`);
    });
}


/**
 * The space the preview is rendered for.
 *
 * A canvas is sRGB unless it is told otherwise, so on a wide-gamut display the
 * render would be squeezed into sRGB and the extra colour simply discarded.
 * Asking for P3 on a screen that cannot show it would be just as wrong — the
 * browser would convert back and the preview would end up slightly desaturated
 * for nothing — so the display's own gamut decides.
 */
export function detectDisplaySpace(): OutputSpaceId {
    if (typeof window === "undefined" || !window.matchMedia) return "srgb";
    return window.matchMedia("(color-gamut: p3)").matches ? "display-p3" : "srgb";
}


// Helper get function to abstract syntax

import { getContext } from "svelte";

export function getGPU(): GPUSession | null {
  const getter = getContext<() => GPUSession | null>("gpu");
  return getter();
}