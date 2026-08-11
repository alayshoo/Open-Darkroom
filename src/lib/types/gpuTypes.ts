// $lib/types/gpuTypes.ts

// Stores the GPU reference and format for display to
// be shared accross svelte components.
export interface GPUSession {
    device: GPUDevice;
    format: GPUTextureFormat;
}


// Stores the linkage between the GPU and the Canvas
export interface GPUCanvasLink {
    canvasConfig : GPUCanvasContext;
}


// Reference to an image loaded on the GPU
export interface GPUImage {
    texture: GPUTexture;
    width: number;
    height: number;
}


// Format of the textures the render chain hands between its stages.
//
// 32-bit float rather than 16: the stages after develop work on differences
// between neighbouring pixels, and a large-radius band is the difference of two
// similar averages. At f16's 11-bit mantissa a band of 0.001 taken from values
// around 0.5 has barely two bits left, which shows up as quantised local
// contrast on smooth gradients. It also keeps the split exact — the develop
// output reaches the final clamp bit for bit.
export const WORKING_FORMAT: GPUTextureFormat = "rgba32float";


// One stage of the render chain: a full-screen quad reading bound resources and
// writing a single target. Develop, composite and the output transform are all
// this shape and differ only in what they bind.
export interface RenderStage {
    label: string;
    pipeline: GPURenderPipeline;
    bindGroupLayout: GPUBindGroupLayout;
}



import type { Sliders } from "./imgParameters";

export interface Renderer {
    loadImage: (image: GPUImage) => void;
    setSliders: (sliders: Sliders) => void;
    setRenderScale: (renderScale: number) => void;
    render: () => void;
    destroy: () => void;
}


export interface HistogramPipeline {
    computeHistogram: (inputTexture: GPUTexture, sliders: Sliders) => Promise<{
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
    }>;
    destroy: () => void;
}
