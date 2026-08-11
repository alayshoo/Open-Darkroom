
import type { GPUSession, GPUCanvasLink } from "$lib/types/gpuTypes";

export async function linkCanvas2GPU(
    canvas: HTMLCanvasElement,
    gpu: GPUSession,
    // "premultiplied" lets whatever is behind the canvas show through where the
    // render leaves alpha at zero — which is how the histogram keeps the panel
    // background it had as a 2D canvas.
    alphaMode: GPUCanvasAlphaMode = "opaque",
): Promise<GPUCanvasLink> {

    const canvasConfig = canvas.getContext("webgpu")!;

    canvasConfig.configure({
        device: gpu.device,
        format: gpu.format,
        alphaMode,
        usage:
            GPUTextureUsage.RENDER_ATTACHMENT |
            GPUTextureUsage.COPY_DST,
    })

    return { canvasConfig };
}