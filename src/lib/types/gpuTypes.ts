// $lib/types/gpuTypes.ts

// Stores the GPU reference and format for display to
// be shared accross svelte components.
export interface GPUSession {
    device: GPUDevice;
    format: GPUTextureFormat;
    // Whether the device was created with `timestamp-query`. Only the frame
    // stats overlay reads it; the render chain does not depend on it.
    canTimestamp: boolean;
    // The adapter behind the device. Kept only so the settings' debug panel can
    // report what the app is actually running on — the adapter itself is not
    // needed once the device exists.
    adapterInfo: GPUAdapterInfo | null;
    // Everything the adapter offers, as opposed to device.features, which is
    // what was asked for.
    adapterFeatures: string[];
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

// Format of the sharpness textures — four blurred lumas in the intermediate,
// three bands and a blurred luma in the output. Same precision as the working
// format, and the precision is the point: a band is the difference of two nearly
// equal averages, which is exactly where a narrower mantissa shows up as
// quantised local contrast on smooth gradients.
export const SHARP_FORMAT: GPUTextureFormat = "rgba32float";


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
    setView: (renderScale: number, fullWidth: number, fullHeight: number) => void;
    // Marks the preview stale. The frame itself is drawn on the next animation
    // frame the GPU has capacity for, so this never blocks the caller.
    requestRender: () => void;
    destroy: () => void;
}


export interface HistogramPipeline {
    // Bins the image, reduces the scale and draws the curves — all on the GPU,
    // in one submit. Omitting the target bins without drawing.
    render: (
        inputTexture: GPUTexture,
        sliders: Sliders,
        target?: GPUTextureView,
    ) => void;
    // The bins on the CPU, at the cost of the round trip drawing on the GPU
    // exists to avoid. For tests.
    readBins: () => Promise<{
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
        // What the last render reduced the bins to for normalisation.
        scale: number;
    }>;
    destroy: () => void;
}
