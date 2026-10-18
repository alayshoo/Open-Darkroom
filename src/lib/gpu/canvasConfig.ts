
import type { GPUSession, GPUCanvasLink } from "$lib/types/gpuTypes";

export async function linkCanvas2GPU(
    canvas: HTMLCanvasElement,
    gpu: GPUSession,
    // "premultiplied" lets whatever is behind the canvas show through where the
    // render leaves alpha at zero — which is how the histogram keeps the panel
    // background it had as a 2D canvas.
    alphaMode: GPUCanvasAlphaMode = "opaque",
    // What the values written to this canvas are taken to mean. Defaults to the
    // display's own gamut, which is what the preview wants: the encode stage
    // applies that space's primaries and curve itself, so the two have to name
    // the same space or the browser converts from the wrong one.
    //
    // A canvas drawing interface rather than the image passes "srgb" and stays
    // there. Its colours are authored to sit next to CSS ones, and a wider
    // canvas would quietly saturate them away from the rest of the UI.
    colorSpace: PredefinedColorSpace = gpu.displaySpace === "display-p3" ? "display-p3" : "srgb",
): Promise<GPUCanvasLink> {

    const canvasConfig = canvas.getContext("webgpu")!;

    canvasConfig.configure({
        device: gpu.device,
        format: gpu.format,
        colorSpace,
        alphaMode,
        usage:
            GPUTextureUsage.RENDER_ATTACHMENT |
            GPUTextureUsage.COPY_DST,
    })

    return { canvasConfig };
}
