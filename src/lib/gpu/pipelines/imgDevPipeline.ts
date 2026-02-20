// src/lib/gpu/pipelines/imgDevPipeline.ts

import type { Adjustments } from "$lib/types/adjustments";
import shaderSource from "../shaders/develop.wgsl?raw";        // read raw data from shader file

export interface ImgDevPipeline {
    pipeline: GPURenderPipeline;
    bindGroupLayout: GPUBindGroupLayout;
    paramsBuffer: GPUBuffer;
    sampler: GPUSampler;
}

export function createImgDevPipeline(
    device: GPUDevice,
    targetFormat: GPUTextureFormat
): ImgDevPipeline {
    // Step 1: Create the shader module from the WGSL source
    const shaderModule = device.createShaderModule({
        label: "Image Development Shader",
        code: shaderSource,
    });

    // Step 2: Define the bind group layout — this tells the pipeline
    // what resources the shader expects and at which bindings.
    const bindGroupLayout = device.createBindGroupLayout({
        label: "Image Development Bind Group Layout",
        entries: [
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
                binding: 2, // params uniform
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type: "uniform" },
            },
        ],
    });

    // Step 3: Create the pipeline layout from our bind group layout(s)
    const pipelineLayout = device.createPipelineLayout({
        bindGroupLayouts: [bindGroupLayout],
    });

    // Step 4: Create the render pipeline — the big one.
    // This compiles the shaders and locks in the configuration.
    const pipeline = device.createRenderPipeline({
        label: "Image Development Pipeline",
        layout: pipelineLayout,
        vertex: {
            module: shaderModule,
            entryPoint: "vs_main",
            // No vertex buffers — we generate positions in the shader
        },
        fragment: {
            module: shaderModule,
            entryPoint: "fs_main",
            targets: [{ format: targetFormat }],
        },
        // No depth/stencil — we're doing 2D image processing
    });

    // Step 5: Create the uniform buffer for our params.
    // 4 bytes for one f32 (exposure). We use a minimum of 16 bytes
    // because WebGPU requires uniform buffers to be 16-byte aligned.
    const paramsBuffer = device.createBuffer({
        label: "Image Development Params",
        size: PARAMS_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    // Step 6: Create a sampler (bilinear filtering, clamp to edge)
    const sampler = device.createSampler({
        magFilter: "linear",
        minFilter: "linear",
        addressModeU: "clamp-to-edge",
        addressModeV: "clamp-to-edge",
    });

    return { pipeline, bindGroupLayout, paramsBuffer, sampler };
}

// Single source of truth for buffer layout.
// Order MUST match the WGSL Params struct.
function paramsToArray(a: Adjustments): Float32Array<ArrayBuffer> {
    const buf = new ArrayBuffer(PARAMS_BUFFER_SIZE);
    const view = new Float32Array(buf);
    view.set([
        a.wb_temp,
        a.wb_tint,
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


// Helper to update the exposure value on the GPU.
// This is what you call when the user moves a slider.
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