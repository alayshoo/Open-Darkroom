// src/lib/gpu/pipelines/imgDevPipeline.ts

import type { GPUSession, ImgDevPipeline } from "$lib/types/gpuTypes"
import shaderSource from "../shaders/develop.wgsl?raw";


export function createImgDevPipeline(
    gpuSession: GPUSession
): ImgDevPipeline {

    // Create the shader module from the WGSL source on the GPU
    const shaderModule = gpuSession.device.createShaderModule({
        label: "Image Development Shader",
        code: shaderSource,
    });

    // Define the bind group layout expected by the shader
    const bindGroupLayout = gpuSession.device.createBindGroupLayout({
        label: "Image Development Bind Group Layout",
        entries: [
            {
                binding: 0, // inputTexture
                visibility: GPUShaderStage.FRAGMENT,
                texture: { sampleType: "float" },
            },
            {
                binding: 1, // textureSampler
                visibility: GPUShaderStage.FRAGMENT,
                sampler: { type: "filtering" },
            },
            {
                binding: 2, // params storage buffer (computed by calcParams shader)
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type: "read-only-storage" },
            },
        ],
    });

    // Create the pipeline layout from the bind group layout
    const pipelineLayout = gpuSession.device.createPipelineLayout({
        bindGroupLayouts: [bindGroupLayout],
    });

    // Create the render pipeline
    // Compiles the shaders to the GPU
    const pipeline = gpuSession.device.createRenderPipeline({
        label: "Image Development Pipeline",
        layout: pipelineLayout,
        vertex: {
            module: shaderModule,
            entryPoint: "vs_main",      // Inject vs_main function
        },
        fragment: {
            module: shaderModule,
            entryPoint: "fs_main",      // Inject fs_main function
            targets: [{ format: gpuSession.format }],
        },
    });

    // Create a sampler (bilinear filtering, clamp to edge)
    const sampler = gpuSession.device.createSampler({
        magFilter: "linear",
        minFilter: "linear",
        addressModeU: "clamp-to-edge",
        addressModeV: "clamp-to-edge",
    });

    return { pipeline, bindGroupLayout, sampler };
}
