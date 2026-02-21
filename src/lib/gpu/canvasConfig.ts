
import type { GPUSession, GPUCanvasLink } from "$lib/types/gpuTypes";

export async function linkCanvas2GPU(
    canvas: HTMLCanvasElement,
    gpu: GPUSession,
): Promise<GPUCanvasLink> {

    const canvasConfig = canvas.getContext("webgpu")!;

    canvasConfig.configure({
        device: gpu.device,
        format: gpu.format,
        usage:
            GPUTextureUsage.RENDER_ATTACHMENT |
            GPUTextureUsage.COPY_DST,
    })

    return { canvasConfig };
}