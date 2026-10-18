// src/lib/gpu/pipelines/colorSpacePipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { GPUSession } from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline } from "./calcParamsPipeline";
import { createFrameTimer } from "./frameTimer";
import { debugStats } from "../debugStats.svelte";
import {
    createGamutOverlayBuffer,
    GAMUT_ORDER,
    type GamutOverlayBuffer,
} from "../gamutOverlay";
import type { OutputSpaceId } from "../outputSpace";
import computeSource from "../shaders/colorSpace.wgsl?raw";
import scaleSource from "../shaders/colorSpaceScale.wgsl?raw";
import chromaticitySource from "../shaders/chromaticityDraw.wgsl?raw";
import volumeSource from "../shaders/colorVolumeDraw.wgsl?raw";

/** Grid sizes, mirroring chunks/gamutGrids.wgsl. */
const XY_TOTAL = 128 * 128;
const HL_TOTAL = 96 * 64 * 2;
const STAT_WORDS = 8;

/** The draws cover only what they reach; the panel shows through the rest. */
const TRANSPARENT: GPUColor = { r: 0, g: 0, b: 0, a: 0 };

export interface GamutStats {
    /** Pixels that carried enough light to have a chromaticity at all. */
    counted: number;
    /** How many of those fell outside each space, by id. */
    outside: Partial<Record<OutputSpaceId, number>>;
}

export interface ColorSpacePipeline {
    /** The open image's primaries, as a matrix into the working space — the
     *  same conversion the preview and the histogram are given, for the same
     *  reason: this runs its own copy of the develop chain. */
    setSourceSpace: (srcToWorking: Float32Array) => void;
    /** Which gamuts the diagram draws, and so which the statistics count. */
    setOverlays: (enabled: Record<OutputSpaceId, boolean>) => void;
    /** Bins the image and draws both plots, in one submit. A target left out is
     *  simply not drawn — the bins are taken either way. */
    render: (
        inputTexture: GPUTexture,
        sliders: Sliders,
        targets: { chromaticity?: GPUTextureView; volume?: GPUTextureView },
    ) => void;
    /** The out-of-gamut tallies, which only mean anything on the CPU.
     *
     *  This is the round trip the histogram exists without, and it costs what
     *  it always did — so the caller asks for it once the plots have settled
     *  rather than on every frame of a slider drag. */
    readStats: () => Promise<GamutStats>;
    destroy: () => void;
}

export function createColorSpacePipeline(gpu: GPUSession): ColorSpacePipeline {
    const { device } = gpu;

    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);
    const overlay: GamutOverlayBuffer = createGamutOverlayBuffer(gpu);

    const xyBins = device.createBuffer({
        label: "Chromaticity Bins",
        size: XY_TOTAL * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    const hlBins = device.createBuffer({
        label: "Colour Volume Bins",
        size: HL_TOTAL * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    const stats = device.createBuffer({
        label: "Gamut Statistics",
        size: STAT_WORDS * 4,
        usage:
            GPUBufferUsage.STORAGE |
            GPUBufferUsage.COPY_SRC |
            GPUBufferUsage.COPY_DST,
    });

    const scale = device.createBuffer({
        label: "Colour Space Scale",
        size: 8,
        usage: GPUBufferUsage.STORAGE,
    });

    // ── Bin: the whole overview through the develop chain ────────────────────
    const computeLayout = device.createBindGroupLayout({
        label: "Colour Space Bind Group Layout",
        entries: [
            { binding: 0, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "float" } },
            { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
            { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
            { binding: 3, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
            { binding: 4, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
            { binding: 5, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        ],
    });

    const computePipeline = device.createComputePipeline({
        label: "Colour Space Compute Pipeline",
        layout: device.createPipelineLayout({ bindGroupLayouts: [computeLayout] }),
        compute: {
            module: device.createShaderModule({
                label: "Colour Space Compute Shader",
                code: computeSource,
            }),
            entryPoint: "main",
        },
    });

    // ── Scale: the fullest cell of each grid ─────────────────────────────────
    const scaleLayout = device.createBindGroupLayout({
        label: "Colour Space Scale Bind Group Layout",
        entries: [
            { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
            { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
            { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        ],
    });

    const scalePipeline = device.createComputePipeline({
        label: "Colour Space Scale Pipeline",
        layout: device.createPipelineLayout({ bindGroupLayouts: [scaleLayout] }),
        compute: {
            module: device.createShaderModule({
                label: "Colour Space Scale Shader",
                code: scaleSource,
            }),
            entryPoint: "main",
        },
    });

    const scaleBindGroup = device.createBindGroup({
        label: "Colour Space Scale Bind Group",
        layout: scaleLayout,
        entries: [
            { binding: 0, resource: { buffer: xyBins } },
            { binding: 1, resource: { buffer: hlBins } },
            { binding: 2, resource: { buffer: scale } },
        ],
    });

    // ── Draw: the two plots, straight from the grids ─────────────────────────
    function drawPipelineFor(label: string, code: string, bindings: number) {
        const layout = device.createBindGroupLayout({
            label: `${label} Bind Group Layout`,
            entries: Array.from({ length: bindings }, (_, binding) => ({
                binding,
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type: "read-only-storage" as const },
            })),
        });

        const module = device.createShaderModule({ label: `${label} Shader`, code });

        return {
            layout,
            pipeline: device.createRenderPipeline({
                label: `${label} Pipeline`,
                layout: device.createPipelineLayout({ bindGroupLayouts: [layout] }),
                vertex: { module, entryPoint: "vs_main" },
                fragment: {
                    module,
                    entryPoint: "fs_main",
                    targets: [{ format: gpu.format }],
                },
            }),
        };
    }

    const chromaticity = drawPipelineFor("Chromaticity Draw", chromaticitySource, 3);
    const volume = drawPipelineFor("Colour Volume Draw", volumeSource, 2);

    const chromaticityBindGroup = device.createBindGroup({
        label: "Chromaticity Draw Bind Group",
        layout: chromaticity.layout,
        entries: [
            { binding: 0, resource: { buffer: xyBins } },
            { binding: 1, resource: { buffer: scale } },
            { binding: 2, resource: { buffer: overlay.buffer } },
        ],
    });

    const volumeBindGroup = device.createBindGroup({
        label: "Colour Volume Draw Bind Group",
        layout: volume.layout,
        entries: [
            { binding: 0, resource: { buffer: hlBins } },
            { binding: 1, resource: { buffer: scale } },
        ],
    });

    const timer = createFrameTimer(gpu);

    // The bind group only changes when the image does, so it is kept rather
    // than rebuilt on every slider move.
    let boundTexture: GPUTexture | null = null;
    let computeBindGroup: GPUBindGroup | null = null;

    function bindGroupFor(inputTexture: GPUTexture): GPUBindGroup {
        if (computeBindGroup && boundTexture === inputTexture) return computeBindGroup;
        boundTexture = inputTexture;
        computeBindGroup = device.createBindGroup({
            label: "Colour Space Bind Group",
            layout: computeLayout,
            entries: [
                { binding: 0, resource: inputTexture.createView() },
                { binding: 1, resource: { buffer: calcParams.paramsBuffer } },
                { binding: 2, resource: { buffer: xyBins } },
                { binding: 3, resource: { buffer: hlBins } },
                { binding: 4, resource: { buffer: stats } },
                { binding: 5, resource: { buffer: overlay.buffer } },
            ],
        });
        return computeBindGroup;
    }

    function recordDraw(
        encoder: GPUCommandEncoder,
        label: string,
        pipeline: GPURenderPipeline,
        bindGroup: GPUBindGroup,
        target: GPUTextureView,
        measure: boolean,
    ) {
        const pass = encoder.beginRenderPass({
            label,
            colorAttachments: [
                { view: target, clearValue: TRANSPARENT, loadOp: "clear", storeOp: "store" },
            ],
            timestampWrites: measure ? timer.passWrites(label) : undefined,
        });
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.draw(6);
        pass.end();
    }

    function setSourceSpace(srcToWorking: Float32Array) {
        calcParams.updateSourceSpace(srcToWorking);
    }

    function setOverlays(enabled: Record<OutputSpaceId, boolean>) {
        overlay.setEnabled(enabled);
    }

    function render(
        inputTexture: GPUTexture,
        sliders: Sliders,
        targets: { chromaticity?: GPUTextureView; volume?: GPUTextureView },
    ) {
        calcParams.updateSliders(sliders);

        const measure = debugStats.enabled;
        const start = measure ? performance.now() : 0;
        if (measure) timer.beginFrame();

        const encoder = device.createCommandEncoder({ label: "Colour Space" });

        calcParams.recordCalcParams(encoder);

        encoder.clearBuffer(xyBins);
        encoder.clearBuffer(hlBins);
        encoder.clearBuffer(stats);

        const pass = encoder.beginComputePass({
            label: "Colour Space Pass",
            timestampWrites: measure ? timer.passWrites("bin") : undefined,
        });
        pass.setPipeline(computePipeline);
        pass.setBindGroup(0, bindGroupFor(inputTexture));
        pass.dispatchWorkgroups(
            Math.ceil(inputTexture.width / 16),
            Math.ceil(inputTexture.height / 16),
        );
        pass.end();

        const scalePass = encoder.beginComputePass({
            label: "Colour Space Scale Pass",
            timestampWrites: measure ? timer.passWrites("scale") : undefined,
        });
        scalePass.setPipeline(scalePipeline);
        scalePass.setBindGroup(0, scaleBindGroup);
        scalePass.dispatchWorkgroups(1);
        scalePass.end();

        if (targets.chromaticity) {
            recordDraw(
                encoder,
                "chromaticity",
                chromaticity.pipeline,
                chromaticityBindGroup,
                targets.chromaticity,
                measure,
            );
        }
        if (targets.volume) {
            recordDraw(encoder, "volume", volume.pipeline, volumeBindGroup, targets.volume, measure);
        }

        if (measure) timer.resolve(encoder);
        device.queue.submit([encoder.finish()]);

        // Nothing is awaited, so this is the CPU cost of recording the work and
        // handing it over — not how long the work took.
        if (measure) {
            const recordMs = performance.now() - start;
            void timer.read().then((passes) => {
                debugStats.recordColorSpace({ passes, recordMs });
            });
        }
    }

    // Allocated on the first readback rather than with the buffers above, so
    // nothing is held for a panel that was never opened.
    let staging: GPUBuffer | null = null;

    async function readStats(): Promise<GamutStats> {
        const target = (staging ??= device.createBuffer({
            label: "Gamut Statistics Staging",
            size: STAT_WORDS * 4,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
        }));

        const encoder = device.createCommandEncoder({ label: "Gamut Statistics Readback" });
        encoder.copyBufferToBuffer(stats, 0, target, 0, STAT_WORDS * 4);
        device.queue.submit([encoder.finish()]);

        await target.mapAsync(GPUMapMode.READ);
        const words = new Uint32Array(target.getMappedRange().slice(0));
        target.unmap();

        const outside: Partial<Record<OutputSpaceId, number>> = {};
        GAMUT_ORDER.forEach((id, slot) => {
            outside[id] = words[1 + slot];
        });

        return { counted: words[0], outside };
    }

    function destroy() {
        calcParams.destroy();
        overlay.destroy();
        timer.destroy();
        xyBins.destroy();
        hlBins.destroy();
        stats.destroy();
        scale.destroy();
        staging?.destroy();
    }

    return { setSourceSpace, setOverlays, render, readStats, destroy };
}
