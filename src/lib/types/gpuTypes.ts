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


// Reference to a compiled shader
export interface ImgDevPipeline {
    pipeline: GPURenderPipeline;
    bindGroupLayout: GPUBindGroupLayout;
    paramsBuffer: GPUBuffer;
    sampler: GPUSampler;
}



import type { Sliders } from "./imgParameters";

export interface Renderer {
    loadImage: (image: GPUImage) => void;
    setSliders: (sliders: Sliders) => void;
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
