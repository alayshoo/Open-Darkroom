
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

    // Request a device — logical handle
    // All GPU objects (buffers, textures, pipelines) belong to a device.
    // Can request specific features/limits here in the future if needed.
    const device = await adapter.requestDevice();

    // Format is the pixel format the display expects.
    const format = navigator.gpu.getPreferredCanvasFormat();

    return { device, format } ;
}


// Helper get function to abstract syntax

import { getContext } from "svelte";

export function getGPU(): GPUSession | null {
  const getter = getContext<() => GPUSession | null>("gpu");
  return getter();
}