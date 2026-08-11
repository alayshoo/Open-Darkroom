// src/lib/gpu/pipelines/chainStages.ts
//
// The three render stages of the develop chain, in order. Each is the generic
// stage factory plus the one thing that distinguishes it: what it binds.
// Declared here rather than inside the renderer so the test suite builds the
// same stages the app does — a layout that drifts from its WGSL is exactly the
// kind of bug the browser layer exists to catch.

import { WORKING_FORMAT, type GPUSession, type RenderStage } from "$lib/types/gpuTypes";
import { createRenderStage, intermediateBinding } from "./renderStagePipeline";
import developSource from "../shaders/develop.wgsl?raw";
import compositeSource from "../shaders/composite.wgsl?raw";
import colorSpaceEncodeSource from "../shaders/colorSpaceEncode.wgsl?raw";

// Steps 1-10, from the source image into the perceptual working space.
export function createDevelopStage(gpu: GPUSession): RenderStage {
    return createRenderStage(gpu, {
        label: "Develop",
        code: developSource,
        format: WORKING_FORMAT,
        bindings: [
            {
                binding: 0, // inputTexture
                visibility: GPUShaderStage.FRAGMENT,
                texture: { sampleType: "float" },
            },
            {
                binding: 1, // texSampler
                visibility: GPUShaderStage.FRAGMENT,
                sampler: { type: "filtering" },
            },
            {
                binding: 2, // params, written by the calcParams compute pass
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type: "read-only-storage" },
            },
        ],
    });
}

// Sharpness bands over the developed signal. Still working space in, working
// space out.
export function createCompositeStage(gpu: GPUSession): RenderStage {
    return createRenderStage(gpu, {
        label: "Composite",
        code: compositeSource,
        format: WORKING_FORMAT,
        bindings: [intermediateBinding(0)],
    });
}

// The output transform — the only stage that knows what it is rendering for, so
// the only one targeting the destination format.
export function createEncodeStage(gpu: GPUSession): RenderStage {
    return createRenderStage(gpu, {
        label: "Colour Space Encode",
        code: colorSpaceEncodeSource,
        format: gpu.format,
        bindings: [intermediateBinding(0)],
    });
}

// The source image is sampled rather than loaded, so the develop stage needs
// this alongside its textures.
export function createSourceSampler(gpu: GPUSession): GPUSampler {
    return gpu.device.createSampler({
        label: "Source Image Sampler",
        magFilter: "linear",
        minFilter: "linear",
        addressModeU: "clamp-to-edge",
        addressModeV: "clamp-to-edge",
    });
}
