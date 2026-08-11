
// lib/gpu/gpu.ts

import { type GPUSession } from "$lib/types/gpuTypes";

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
    const device = await adapter.requestDevice({
        requiredFeatures: canTimestamp ? ["timestamp-query"] : [],
    });

    // Format is the pixel format the display expects.
    const format = navigator.gpu.getPreferredCanvasFormat();

    return { device, format, canTimestamp } ;
}


// Helper get function to abstract syntax

import { getContext } from "svelte";

export function getGPU(): GPUSession | null {
  const getter = getContext<() => GPUSession | null>("gpu");
  return getter();
}