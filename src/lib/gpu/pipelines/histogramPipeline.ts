// src/lib/gpu/pipelines/histogramPipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { GPUSession, HistogramPipeline } from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline, PARAMS_BUFFER_SIZE } from "./calcParamsPipeline";
import shaderSource from "../shaders/histogram.wgsl?raw";

/** Number of bins per channel (matches the shader's array<atomic<u32>, 256>). */
const NUM_BINS = 256;

/** Size in bytes of one channel's bin buffer (256 × 4 bytes per u32). */
const BINS_BUFFER_SIZE = NUM_BINS * 4;

/**
 * Creates the histogram compute pipeline.
 *
 * Returns an object with:
 *  - `computeHistogram(inputTexture, sliders)` — dispatches the compute
 *     shader, reads back the 3×256 bin arrays from the GPU, and returns them.
 *  - `destroy()` — releases all GPU resources owned by this pipeline.
 */
export function createHistogramPipeline(gpu: GPUSession): HistogramPipeline {
    const { device } = gpu;

    // ── Calc params pipeline (computes matrices on GPU) ───────────
    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);

    // ── Shader module ────────────────────────────────────────────────
    const shaderModule = device.createShaderModule({
        label: "Histogram Compute Shader",
        code: shaderSource,
    });

    // ── Bind group layout (must match histogram.wgsl bindings) ───────
    const bindGroupLayout = device.createBindGroupLayout({
        label: "Histogram Bind Group Layout",
        entries: [
            {
                binding: 0, // inputTexture
                visibility: GPUShaderStage.COMPUTE,
                texture: { sampleType: "float" },
            },
            {
                binding: 1, // params storage buffer (computed by calcParams shader)
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "read-only-storage" },
            },
            {
                binding: 2, // bins_r
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "storage" },
            },
            {
                binding: 3, // bins_g
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "storage" },
            },
            {
                binding: 4, // bins_b
                visibility: GPUShaderStage.COMPUTE,
                buffer: { type: "storage" },
            },
        ],
    });

    // ── Pipeline ─────────────────────────────────────────────────────
    const pipelineLayout = device.createPipelineLayout({
        bindGroupLayouts: [bindGroupLayout],
    });

    const pipeline = device.createComputePipeline({
        label: "Histogram Compute Pipeline",
        layout: pipelineLayout,
        compute: {
            module: shaderModule,
            entryPoint: "main",
        },
    });

    // ── Storage buffers for histogram bins (GPU-side) ────────────────
    // These are written to atomically by the compute shader and then
    // copied to staging buffers for CPU readback.
    const binsR = device.createBuffer({
        label: "Histogram Bins R",
        size: BINS_BUFFER_SIZE,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |
            GPUBufferUsage.COPY_DST,  // needed for clearBuffer
    });

    const binsG = device.createBuffer({
        label: "Histogram Bins G",
        size: BINS_BUFFER_SIZE,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |
            GPUBufferUsage.COPY_DST,
    });

    const binsB = device.createBuffer({
        label: "Histogram Bins B",
        size: BINS_BUFFER_SIZE,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |
            GPUBufferUsage.COPY_DST,
    });

    // ── Staging buffers (MAP_READ) for GPU → CPU readback ────────────
    const stagingR = device.createBuffer({
        label: "Histogram Staging R",
        size: BINS_BUFFER_SIZE,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });

    const stagingG = device.createBuffer({
        label: "Histogram Staging G",
        size: BINS_BUFFER_SIZE,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });

    const stagingB = device.createBuffer({
        label: "Histogram Staging B",
        size: BINS_BUFFER_SIZE,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });

    // ── Public API ───────────────────────────────────────────────────

    async function computeHistogram(
        inputTexture: GPUTexture,
        sliders: Sliders,
    ): Promise<{ r: Uint32Array; g: Uint32Array; b: Uint32Array }> {
        // Upload raw slider values to the GPU
        calcParams.updateSliders(sliders);

        // Create a bind group connecting the actual resources
        const bindGroup = device.createBindGroup({
            label: "Histogram Bind Group",
            layout: bindGroupLayout,
            entries: [
                { binding: 0, resource: inputTexture.createView() },
                { binding: 1, resource: { buffer: calcParams.paramsBuffer } },
                { binding: 2, resource: { buffer: binsR } },
                { binding: 3, resource: { buffer: binsG } },
                { binding: 4, resource: { buffer: binsB } },
            ],
        });

        const encoder = device.createCommandEncoder({
            label: "Histogram Compute",
        });

        // First: compute params from sliders (matrices calculated on GPU)
        calcParams.recordCalcParams(encoder);

        // Clear bins to zero before dispatch
        encoder.clearBuffer(binsR);
        encoder.clearBuffer(binsG);
        encoder.clearBuffer(binsB);

        // Dispatch histogram compute shader
        const pass = encoder.beginComputePass({ label: "Histogram Pass" });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);

        // Workgroups: ceil(width / 16) × ceil(height / 16)
        const workgroupsX = Math.ceil(inputTexture.width / 16);
        const workgroupsY = Math.ceil(inputTexture.height / 16);
        pass.dispatchWorkgroups(workgroupsX, workgroupsY);
        pass.end();

        // Copy bins → staging buffers for CPU readback
        encoder.copyBufferToBuffer(binsR, 0, stagingR, 0, BINS_BUFFER_SIZE);
        encoder.copyBufferToBuffer(binsG, 0, stagingG, 0, BINS_BUFFER_SIZE);
        encoder.copyBufferToBuffer(binsB, 0, stagingB, 0, BINS_BUFFER_SIZE);

        device.queue.submit([encoder.finish()]);

        // Map staging buffers to CPU
        await Promise.all([
            stagingR.mapAsync(GPUMapMode.READ),
            stagingG.mapAsync(GPUMapMode.READ),
            stagingB.mapAsync(GPUMapMode.READ),
        ]);

        // Copy the data out (mapAsync gives a temporary ArrayBuffer view)
        const r = new Uint32Array(stagingR.getMappedRange().slice(0));
        const g = new Uint32Array(stagingG.getMappedRange().slice(0));
        const b = new Uint32Array(stagingB.getMappedRange().slice(0));

        stagingR.unmap();
        stagingG.unmap();
        stagingB.unmap();

        return { r, g, b };
    }

    function destroy() {
        calcParams.destroy();
        binsR.destroy();
        binsG.destroy();
        binsB.destroy();
        stagingR.destroy();
        stagingG.destroy();
        stagingB.destroy();
    }

    return { computeHistogram, destroy };
}
