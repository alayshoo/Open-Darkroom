// src/lib/gpu/pipelines/histogramPipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { GPUSession, HistogramPipeline } from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline, PARAMS_BUFFER_SIZE } from "./calcParamsPipeline";
import { createFrameTimer } from "./frameTimer";
import { debugStats } from "../debugStats.svelte";
import shaderSource from "../shaders/histogram.wgsl?raw";

/** Number of bins per channel (matches the shader's array<atomic<u32>, 256>). */
const NUM_BINS = 256;

/** Size in bytes of one channel's bin buffer (256 × 4 bytes per u32). */
/** All three channels end to end, as `bins` in histogram.wgsl declares them. */
const BINS_BUFFER_SIZE = NUM_BINS * 3 * 4;

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
                binding: 2, // bins — all three channels, end to end
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

    // ── Storage buffer for histogram bins (GPU-side) ─────────────────
    // Written atomically by the compute shader, then copied to staging for CPU
    // readback. All three channels share one buffer: they are always produced
    // and consumed together, so splitting them only multiplies the copies and
    // the maps.
    const bins = device.createBuffer({
        label: "Histogram Bins",
        size: BINS_BUFFER_SIZE,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |
            GPUBufferUsage.COPY_DST,  // needed for clearBuffer
    });

    // ── Staging buffer (MAP_READ) for GPU → CPU readback ─────────────
    const staging = device.createBuffer({
        label: "Histogram Staging",
        size: BINS_BUFFER_SIZE,
        usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });

    // Times the compute pass on its own, so the shader's cost can be told apart
    // from the readback round trip that follows it.
    const timer = createFrameTimer(gpu);

    // The bind group only changes when the image does, so it is kept rather
    // than rebuilt on every slider move.
    let boundTexture: GPUTexture | null = null;
    let bindGroup: GPUBindGroup | null = null;

    function bindGroupFor(inputTexture: GPUTexture): GPUBindGroup {
        if (bindGroup && boundTexture === inputTexture) return bindGroup;
        boundTexture = inputTexture;
        bindGroup = device.createBindGroup({
            label: "Histogram Bind Group",
            layout: bindGroupLayout,
            entries: [
                { binding: 0, resource: inputTexture.createView() },
                { binding: 1, resource: { buffer: calcParams.paramsBuffer } },
                { binding: 2, resource: { buffer: bins } },
            ],
        });
        return bindGroup;
    }

    // ── Public API ───────────────────────────────────────────────────

    async function computeHistogram(
        inputTexture: GPUTexture,
        sliders: Sliders,
    ): Promise<{ r: Uint32Array; g: Uint32Array; b: Uint32Array }> {
        // Upload raw slider values to the GPU
        calcParams.updateSliders(sliders);

        const measure = debugStats.enabled;
        const start = measure ? performance.now() : 0;
        if (measure) timer.beginFrame();

        const bindGroup = bindGroupFor(inputTexture);

        const encoder = device.createCommandEncoder({
            label: "Histogram Compute",
        });

        // First: compute params from sliders (matrices calculated on GPU)
        calcParams.recordCalcParams(encoder);

        // Clear bins to zero before dispatch
        encoder.clearBuffer(bins);

        // Dispatch histogram compute shader
        const pass = encoder.beginComputePass({
            label: "Histogram Pass",
            timestampWrites: measure ? timer.passWrites("histogram") : undefined,
        });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);

        // Workgroups: ceil(width / 16) × ceil(height / 16)
        const workgroupsX = Math.ceil(inputTexture.width / 16);
        const workgroupsY = Math.ceil(inputTexture.height / 16);
        pass.dispatchWorkgroups(workgroupsX, workgroupsY);
        pass.end();

        // Copy bins → staging for CPU readback
        encoder.copyBufferToBuffer(bins, 0, staging, 0, BINS_BUFFER_SIZE);
        if (measure) timer.resolve(encoder);

        device.queue.submit([encoder.finish()]);

        await staging.mapAsync(GPUMapMode.READ);

        // Copy the data out (mapAsync gives a temporary ArrayBuffer view)
        const all = new Uint32Array(staging.getMappedRange().slice(0));
        staging.unmap();

        // Stopped here rather than after the timings are read: this is the point
        // the caller can use the data, and the pass timing is a separate map
        // that would otherwise be counted as part of the wait.
        const wallMs = measure ? performance.now() - start : 0;
        if (measure) {
            void timer.read().then((passes) => {
                debugStats.recordHistogram({ gpuMs: passes[0]?.ms ?? 0, wallMs });
            });
        }

        return {
            r: all.slice(0, NUM_BINS),
            g: all.slice(NUM_BINS, NUM_BINS * 2),
            b: all.slice(NUM_BINS * 2, NUM_BINS * 3),
        };
    }

    function destroy() {
        calcParams.destroy();
        timer.destroy();
        bins.destroy();
        staging.destroy();
    }

    return { computeHistogram, destroy };
}
