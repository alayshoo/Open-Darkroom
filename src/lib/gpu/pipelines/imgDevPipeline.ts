// src/lib/gpu/pipelines/imgDevPipeline.ts

import type { Adjustments } from "$lib/types/adjustments";
import type { GPUSession, ImgDevPipeline } from "$lib/types/gpuTypes"
import shaderSource from "../shaders/develop.wgsl?raw";        // read raw data from shader file


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
                binding: 2, // params uniform
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type: "uniform" },
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

    // Create the uniform buffer in VRAM for the parameters.
    // 4 bytes for one f32. Minimum of 16 bytes because
    // WebGPU requires uniform buffers to be 16-byte aligned.
    const paramsBuffer = gpuSession.device.createBuffer({
        label: "Image Development Params",
        size: PARAMS_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    // Create a sampler (bilinear filtering, clamp to edge)
    const sampler = gpuSession.device.createSampler({
        magFilter: "linear",
        minFilter: "linear",
        addressModeU: "clamp-to-edge",
        addressModeV: "clamp-to-edge",
    });

    return { pipeline, bindGroupLayout, paramsBuffer, sampler };
}

// Single source of truth for buffer layout.
function paramsToArray(a: Adjustments): Float32Array<ArrayBuffer> {
    const buf = new ArrayBuffer(PARAMS_BUFFER_SIZE);
    const view = new Float32Array(buf);
    view.set([
        a.wbTemp,
        a.wbTint,
        a.exposure,
        a.contrast,
        a.brightness,
        a.saturation,
        a.vibrance,
        0, // _pad
    ]);
    return view;
  }

// Buffer size: must be multiple of 16
export const PARAMS_BUFFER_SIZE = 32;


// Helper to update the parameters values on the GPU
export function updateParams(
    device: GPUDevice,
    paramsBuffer: GPUBuffer,
    params: Adjustments
) {
    // writeBuffer copies CPU data → GPU buffer
    device.queue.writeBuffer(
        paramsBuffer,
        0,
        paramsToArray(params)
    );
}