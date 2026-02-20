// src/lib/gpu/context.ts

export interface GPUContext {
    device: GPUDevice;
    context: GPUCanvasContext;
    format: GPUTextureFormat;
}

export async function initGPU(
    canvas: HTMLCanvasElement
): Promise<GPUContext> {
    // Check if WebGPU is available
    if (!navigator.gpu) {
        throw new Error("WebGPU not supported in this browser");
    }

    // Request an adapter — this is the physical GPU.
    // Returns null if no suitable GPU is found.
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) {
        throw new Error("No GPU adapter found");
    }

    // Request a device — logical handle
    // All GPU objects (buffers, textures, pipelines) belong to a device.
    // Can request specific features/limits here if needed.
    const device = await adapter.requestDevice();

    // Configure the canvas for WebGPU output.
    // The "format" is the pixel format the display expects —
    // typically "bgra8unorm" on most systems.
    const context = canvas.getContext("webgpu")!;
    const format = navigator.gpu.getPreferredCanvasFormat();

    context.configure({
        device,
        format,
        // This allows the output texture    be used as a render target
        // AND as an input to compute shaders if needed later.
        usage:
            GPUTextureUsage.RENDER_ATTACHMENT |
            GPUTextureUsage.COPY_DST,
    });

    return { device, context, format };
}