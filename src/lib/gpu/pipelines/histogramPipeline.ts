// src/lib/gpu/pipelines/histogramPipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { GPUSession, HistogramPipeline } from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline, PARAMS_BUFFER_SIZE } from "./calcParamsPipeline";
import { createFrameTimer } from "./frameTimer";
import { debugStats } from "../debugStats.svelte";
import shaderSource from "../shaders/histogram.wgsl?raw";
import scaleSource from "../shaders/histogramScale.wgsl?raw";
import drawSource from "../shaders/histogramDraw.wgsl?raw";

/** Number of bins per channel (matches the shader's array<atomic<u32>, 256>). */
const NUM_BINS = 256;

/** All three channels end to end, as `bins` in histogram.wgsl declares them. */
const BINS_BUFFER_SIZE = NUM_BINS * 3 * 4;

/** The draw covers only what the curves reach; the panel shows through the rest. */
const TRANSPARENT: GPUColor = { r: 0, g: 0, b: 0, a: 0 };

/**
 * Bins an image and draws the result, without the bins ever reaching the CPU.
 *
 * Reading them back cost a GPU→CPU map — 15-25ms of queue drain and callback
 * dispatch against a compute pass of 0.6ms, and a fence per update on the queue
 * the preview renders on. Normalising and drawing on the GPU removes the round
 * trip rather than making it cheaper or rarer, which is the only thing that
 * ever reduced it.
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
    // Written atomically by the compute shader, then read in place by the scale
    // and draw passes. All three channels share one buffer: they are always
    // produced and consumed together, so splitting them would only multiply the
    // clears and the bindings.
    const bins = device.createBuffer({
        label: "Histogram Bins",
        size: BINS_BUFFER_SIZE,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |  // readBins
            GPUBufferUsage.COPY_DST,   // clearBuffer
    });

    // ── Scale: the tallest bin, reduced on the GPU ───────────────────
    const scaleBuffer = device.createBuffer({
        label: "Histogram Scale",
        size: 4,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC,  // readBins
    });

    const scaleLayout = device.createBindGroupLayout({
        label: "Histogram Scale Bind Group Layout",
        entries: [
            { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
            { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        ],
    });

    const scalePipeline = device.createComputePipeline({
        label: "Histogram Scale Pipeline",
        layout: device.createPipelineLayout({ bindGroupLayouts: [scaleLayout] }),
        compute: {
            module: device.createShaderModule({
                label: "Histogram Scale Shader",
                code: scaleSource,
            }),
            entryPoint: "main",
        },
    });

    // ── Draw: the curves, straight from the bins ─────────────────────
    const drawLayout = device.createBindGroupLayout({
        label: "Histogram Draw Bind Group Layout",
        entries: [
            { binding: 0, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "read-only-storage" } },
            { binding: 1, visibility: GPUShaderStage.FRAGMENT, buffer: { type: "read-only-storage" } },
        ],
    });

    const drawModule = device.createShaderModule({
        label: "Histogram Draw Shader",
        code: drawSource,
    });

    const drawPipeline = device.createRenderPipeline({
        label: "Histogram Draw Pipeline",
        layout: device.createPipelineLayout({ bindGroupLayouts: [drawLayout] }),
        vertex: { module: drawModule, entryPoint: "vs_main" },
        fragment: {
            module: drawModule,
            entryPoint: "fs_main",
            targets: [{ format: gpu.format }],
        },
    });

    // Both read the same two buffers for the life of the pipeline, so neither
    // group ever needs rebuilding.
    const scaleBindGroup = device.createBindGroup({
        label: "Histogram Scale Bind Group",
        layout: scaleLayout,
        entries: [
            { binding: 0, resource: { buffer: bins } },
            { binding: 1, resource: { buffer: scaleBuffer } },
        ],
    });

    const drawBindGroup = device.createBindGroup({
        label: "Histogram Draw Bind Group",
        layout: drawLayout,
        entries: [
            { binding: 0, resource: { buffer: bins } },
            { binding: 1, resource: { buffer: scaleBuffer } },
        ],
    });

    // Times the compute pass on its own, against the whole update.
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

    function render(
        inputTexture: GPUTexture,
        sliders: Sliders,
        target?: GPUTextureView,
    ) {
        // Upload raw slider values to the GPU
        calcParams.updateSliders(sliders);

        const measure = debugStats.enabled;
        const start = measure ? performance.now() : 0;
        if (measure) timer.beginFrame();

        const encoder = device.createCommandEncoder({
            label: "Histogram",
        });

        // First: compute params from sliders (matrices calculated on GPU)
        calcParams.recordCalcParams(encoder);

        // Clear bins to zero before dispatch
        encoder.clearBuffer(bins);

        // Dispatch histogram compute shader
        const pass = encoder.beginComputePass({
            label: "Histogram Pass",
            timestampWrites: measure ? timer.passWrites("bin") : undefined,
        });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroupFor(inputTexture));

        // Workgroups: ceil(width / 16) × ceil(height / 16)
        const workgroupsX = Math.ceil(inputTexture.width / 16);
        const workgroupsY = Math.ceil(inputTexture.height / 16);
        pass.dispatchWorkgroups(workgroupsX, workgroupsY);
        pass.end();

        // Reduce the bins to the value the draw normalises against. One group,
        // and the answer stays in a buffer the draw reads directly.
        const scalePass = encoder.beginComputePass({
            label: "Histogram Scale Pass",
            timestampWrites: measure ? timer.passWrites("scale") : undefined,
        });
        scalePass.setPipeline(scalePipeline);
        scalePass.setBindGroup(0, scaleBindGroup);
        scalePass.dispatchWorkgroups(1);
        scalePass.end();

        if (target) {
            const drawPass = encoder.beginRenderPass({
                label: "Histogram Draw Pass",
                colorAttachments: [
                    {
                        view: target,
                        clearValue: TRANSPARENT,
                        loadOp: "clear",
                        storeOp: "store",
                    },
                ],
                timestampWrites: measure ? timer.passWrites("draw") : undefined,
            });
            drawPass.setPipeline(drawPipeline);
            drawPass.setBindGroup(0, drawBindGroup);
            drawPass.draw(6);
            drawPass.end();
        }

        if (measure) timer.resolve(encoder);
        device.queue.submit([encoder.finish()]);

        // Nothing is awaited, so this is the CPU cost of recording and handing
        // over the work — not how long the work took.
        if (measure) {
            const recordMs = performance.now() - start;
            void timer.read().then((passes) => {
                debugStats.recordHistogram({ passes, recordMs });
            });
        }
    }

    // Allocated on the first readback rather than with the buffers above, so a
    // pipeline that never reads back never carries one — which is every
    // pipeline the app makes.
    let staging: GPUBuffer | null = null;

    /**
     * The bins, on the CPU.
     *
     * Nothing in the app calls this — the whole point of drawing on the GPU is
     * that the round trip is gone. It exists so the tests can still check what
     * the binning produced, and it costs what it always did, so anything that
     * needs it in future is choosing to pay that again.
     */
    async function readBins(): Promise<{
        r: Uint32Array;
        g: Uint32Array;
        b: Uint32Array;
        scale: number;
    }> {
        const target = (staging ??= device.createBuffer({
            label: "Histogram Staging",
            // The bins, with the reduced scale appended after them.
            size: BINS_BUFFER_SIZE + 4,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
        }));

        const encoder = device.createCommandEncoder({ label: "Histogram Readback" });
        encoder.copyBufferToBuffer(bins, 0, target, 0, BINS_BUFFER_SIZE);
        encoder.copyBufferToBuffer(scaleBuffer, 0, target, BINS_BUFFER_SIZE, 4);
        device.queue.submit([encoder.finish()]);

        await target.mapAsync(GPUMapMode.READ);
        const all = new Uint32Array(target.getMappedRange().slice(0));
        target.unmap();

        return {
            r: all.slice(0, NUM_BINS),
            g: all.slice(NUM_BINS, NUM_BINS * 2),
            b: all.slice(NUM_BINS * 2, NUM_BINS * 3),
            scale: all[NUM_BINS * 3],
        };
    }

    function destroy() {
        calcParams.destroy();
        timer.destroy();
        bins.destroy();
        scaleBuffer.destroy();
        staging?.destroy();
    }

    return { render, readBins, destroy };
}
