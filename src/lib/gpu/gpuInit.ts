
// lib/gpu/gpu.ts

import { type GPUSession } from "$lib/types/gpuTypes";
import type { OutputSpaceId } from "./outputSpace";

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

    return {
        device,
        format,
        displaySpace: detectDisplaySpace(),
        canTimestamp,
        adapterInfo: adapter.info ?? null,
        adapterFeatures: [...adapter.features].sort(),
    };
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
function detectDisplaySpace(): OutputSpaceId {
    if (typeof window === "undefined" || !window.matchMedia) return "srgb";
    return window.matchMedia("(color-gamut: p3)").matches ? "display-p3" : "srgb";
}


// Helper get function to abstract syntax

import { getContext } from "svelte";

export function getGPU(): GPUSession | null {
  const getter = getContext<() => GPUSession | null>("gpu");
  return getter();
}