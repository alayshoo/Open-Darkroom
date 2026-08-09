// src/lib/gpu/pipelines/calcParamsPipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { GPUSession } from "$lib/types/gpuTypes";
import shaderSource from "../shaders/calcParams.wgsl?raw";

// 30 f32 fields = 120 bytes, rounded up to the 16-byte alignment a uniform
// struct is bound at.
export const SLIDERS_BUFFER_SIZE = 128;
export const PARAMS_BUFFER_SIZE  = 256;  // Params struct (must match shader)
export const VIEW_BUFFER_SIZE    = 16;   // View struct (one f32, 16-aligned)

export interface CalcParamsPipeline {
    paramsBuffer: GPUBuffer;
    updateSliders: (sliders: Sliders) => void;
    updateView: (renderScale: number) => void;
    recordCalcParams: (encoder: GPUCommandEncoder) => void;
    destroy: () => void;
}

export function createCalcParamsPipeline(gpu: GPUSession): CalcParamsPipeline {
    const { device } = gpu;

    const shaderModule = device.createShaderModule({
        label: "Calc Params Compute Shader",
        code: shaderSource,
    });

    // Sliders uniform buffer — raw slider values uploaded from CPU
    const slidersBuffer = device.createBuffer({
        label: "Sliders Uniform Buffer",
        size: SLIDERS_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    // Params storage buffer — computed matrices written by the shader,
    // read by develop.wgsl / histogram.wgsl
    const paramsBuffer = device.createBuffer({
        label: "Computed Params Storage Buffer",
        size: PARAMS_BUFFER_SIZE,
        usage: GPUBufferUsage.STORAGE,
    });

    // View uniform buffer — where this render sits relative to the full image
    const viewBuffer = device.createBuffer({
        label: "View Uniform Buffer",
        size: VIEW_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    const bindGroupLayout = device.createBindGroupLayout({
        label: "Calc Params Bind Group Layout",
        entries: [
            {
                binding: 0,
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "uniform" },
            },
            {
                binding: 1,
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "storage" },
            },
            {
                binding: 2,
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "uniform" },
            },
        ],
    });

    const pipeline = device.createComputePipeline({
        label: "Calc Params Compute Pipeline",
        layout: device.createPipelineLayout({
            bindGroupLayouts: [bindGroupLayout],
        }),
        compute: {
            module: shaderModule,
            entryPoint: "main",
        },
    });

    const bindGroup = device.createBindGroup({
        label: "Calc Params Bind Group",
        layout: bindGroupLayout,
        entries: [
            { binding: 0, resource: { buffer: slidersBuffer } },
            { binding: 1, resource: { buffer: paramsBuffer } },
            { binding: 2, resource: { buffer: viewBuffer } },
        ],
    });

    function updateSliders(sliders: Sliders) {
        const data = slidersToArray(sliders);
        device.queue.writeBuffer(slidersBuffer, 0, data.buffer, data.byteOffset, data.byteLength);
    }

    function updateView(renderScale: number) {
        const data = new Float32Array([renderScale, 0, 0, 0]);
        device.queue.writeBuffer(viewBuffer, 0, data.buffer, data.byteOffset, data.byteLength);
    }

    function recordCalcParams(encoder: GPUCommandEncoder) {
        const pass = encoder.beginComputePass({ label: "Calc Params Pass" });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.dispatchWorkgroups(1);
        pass.end();
    }

    function destroy() {
        slidersBuffer.destroy();
        paramsBuffer.destroy();
        viewBuffer.destroy();
    }

    // A render is full resolution until something says otherwise.
    updateView(1);

    return { paramsBuffer, updateSliders, updateView, recordCalcParams, destroy };
}

// Packs raw slider values into a Float32Array matching the WGSL Sliders struct
function slidersToArray(s: Sliders): Float32Array {
    return new Float32Array([
        s.invert ? 1.0 : 0.0,
        s.redBlackPoint,
        s.greenBlackPoint,
        s.blueBlackPoint,
        s.redWhitePoint,
        s.greenWhitePoint,
        s.blueWhitePoint,
        s.rgbOutputBlack,
        s.rgbOutputWhite,
        s.redGamma,
        s.greenGamma,
        s.blueGamma,
        s.wbTemp,
        s.wbTint,
        s.exposure,
        s.contrast,
        s.brightness,
        s.highlights,
        s.shadows,
        s.whites,
        s.blacks,
        s.saturation,
        s.vibrance,
        s.hue,
        s.clarity,
        s.texture,
        s.usmAmount,
        s.usmRadius,
        s.usmLumaThreshold,
        s.usmDetailThreshold,
    ]);
}
