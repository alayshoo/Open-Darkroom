// src/lib/gpu/pipelines/colorSpacePipeline.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { ToneMap } from "$lib/types/toneMap";
import type { GPUSession } from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline } from "./calcParamsPipeline";
import { createFrameTimer } from "./frameTimer";
import { debugStats } from "../debugStats.svelte";
import {
    createGamutOverlayBuffer,
    GAMUT_ORDER,
    type GamutOverlayBuffer,
    type GamutOverlayState,
} from "../gamutOverlay";
import { createOutputSpaceBuffer } from "../outputSpaceBuffer";
import type { OutputSpaceId } from "../outputSpace";
import computeSource from "../shaders/colorSpace.wgsl?raw";
import scaleSource from "../shaders/colorSpaceScale.wgsl?raw";
import chromaticitySource from "../shaders/chromaticityDraw.wgsl?raw";
import volumeSource from "../shaders/colorVolumeDraw.wgsl?raw";

/** Grid sizes, mirroring chunks/gamutGrids.wgsl. */
const XY_TOTAL = 128 * 128;
const VOL_TOTAL = 48 * 48 * 48;
const STAT_WORDS = 8;

/** The `Camera` uniform in colorVolumeDraw.wgsl: four floats, padded to 16. */
const CAMERA_BUFFER_SIZE = 16;

/** Where the colour volume is being looked at from. */
export interface VolumeView {
    yaw: number;
    pitch: number;
    /** Width over height of the target, so the box stays a box. */
    aspect: number;
}

/** The draws cover only what they reach; the panel shows through the rest. */
const TRANSPARENT: GPUColor = { r: 0, g: 0, b: 0, a: 0 };

export interface GamutStats {
    /** Pixels that carried enough light to have a chromaticity at all. */
    counted: number;
    /** How many of those fell outside each space, by id. */
    outside: Partial<Record<OutputSpaceId, number>>;
}

export interface Targets {
    chromaticity?: GPUTextureView;
    volume?: GPUTextureView;
}

export interface ColorSpacePipeline {
    /** The open image's source → working space matrix. */
    setSourceSpace: (srcToWorking: Float32Array) => void;
    /** Which gamuts are drawn and counted, and what is measured. */
    setOverlays: (state: GamutOverlayState) => void;
    /** The output transform's destination, for measuring after it. */
    setOutputSpace: (destination: OutputSpaceId, proof: OutputSpaceId | null) => void;
    /** The highlight roll-off preference the preview renders with. */
    setToneMap: (toneMap: ToneMap) => void;
    /** The baked point curves, or null while every curve is the identity. */
    setCurves: (lut: Float32Array | null) => void;
    /** Where the colour volume is being looked at from. */
    setVolumeView: (view: VolumeView) => void;
    /** Bins the image and draws both plots. A target left out is not drawn. */
    render: (inputTexture: GPUTexture, sliders: Sliders, targets: Targets) => void;
    /** Draws the plots again from the existing bins. */
    redraw: (targets: Targets) => void;
    /** Reads the out-of-gamut tallies back. Only one read may be in flight. */
    readStats: () => Promise<GamutStats>;
    destroy: () => void;
}

export function createColorSpacePipeline(gpu: GPUSession): ColorSpacePipeline {
    const { device } = gpu;

    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);
    const overlay: GamutOverlayBuffer = createGamutOverlayBuffer(gpu);
    const outputSpace = createOutputSpaceBuffer(gpu);

    const camera = device.createBuffer({
        label: "Colour Volume Camera",
        size: CAMERA_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    const xyBins = device.createBuffer({
        label: "Chromaticity Bins",
        size: XY_TOTAL * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    const volBins = device.createBuffer({
        label: "Colour Volume Bins",
        size: VOL_TOTAL * 4,
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
            { binding: 6, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
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
            { binding: 1, resource: { buffer: volBins } },
            { binding: 2, resource: { buffer: scale } },
        ],
    });

    // ── Draw: the two plots, straight from the grids ─────────────────────────
    function drawPipelineFor(label: string, code: string, bindings: GPUBufferBindingType[]) {
        const layout = device.createBindGroupLayout({
            label: `${label} Bind Group Layout`,
            entries: bindings.map((type, binding) => ({
                binding,
                visibility: GPUShaderStage.FRAGMENT,
                buffer: { type },
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

    const chromaticity = drawPipelineFor("Chromaticity Draw", chromaticitySource, [
        "read-only-storage",
        "read-only-storage",
        "read-only-storage",
    ]);
    const volume = drawPipelineFor("Colour Volume Draw", volumeSource, [
        "read-only-storage",
        "read-only-storage",
        "uniform",
    ]);

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
            { binding: 0, resource: { buffer: volBins } },
            { binding: 1, resource: { buffer: scale } },
            { binding: 2, resource: { buffer: camera } },
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
                { binding: 3, resource: { buffer: volBins } },
                { binding: 4, resource: { buffer: stats } },
                { binding: 5, resource: { buffer: overlay.buffer } },
                { binding: 6, resource: { buffer: outputSpace.buffer } },
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

    function setOverlays(state: GamutOverlayState) {
        overlay.set(state);
    }

    function setOutputSpace(destination: OutputSpaceId, proof: OutputSpaceId | null) {
        outputSpace.set(destination, proof);
    }

    function setToneMap(toneMap: ToneMap) {
        calcParams.updateToneMap(toneMap);
    }

    function setCurves(lut: Float32Array | null) {
        calcParams.updateCurves(lut);
    }

    const cameraStaging = new Float32Array(CAMERA_BUFFER_SIZE / 4);

    function setVolumeView(view: VolumeView) {
        cameraStaging[0] = view.yaw;
        cameraStaging[1] = view.pitch;
        cameraStaging[2] = view.aspect;
        // Half the box's extent in clip units, enough to keep every corner in view.
        cameraStaging[3] = 0.95;
        device.queue.writeBuffer(camera, 0, cameraStaging);
    }

    function recordDraws(encoder: GPUCommandEncoder, targets: Targets, measure: boolean) {
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
    }

    /** The timings both entry points report, which differ only in their label. */
    function finish(encoder: GPUCommandEncoder, measure: boolean, start: number) {
        if (measure) timer.resolve(encoder);
        device.queue.submit([encoder.finish()]);

        // CPU cost of recording and submitting, not GPU time.
        if (measure) {
            const recordMs = performance.now() - start;
            void timer.read().then((passes) => {
                debugStats.recordColorSpace({ passes, recordMs });
            });
        }
    }

    function render(inputTexture: GPUTexture, sliders: Sliders, targets: Targets) {
        calcParams.updateSliders(sliders);

        const measure = debugStats.enabled;
        const start = measure ? performance.now() : 0;
        if (measure) timer.beginFrame();

        const encoder = device.createCommandEncoder({ label: "Colour Space" });

        calcParams.recordCalcParams(encoder);

        encoder.clearBuffer(xyBins);
        encoder.clearBuffer(volBins);
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

        recordDraws(encoder, targets, measure);
        finish(encoder, measure, start);
    }

    function redraw(targets: Targets) {
        const measure = debugStats.enabled;
        const start = measure ? performance.now() : 0;
        if (measure) timer.beginFrame();

        const encoder = device.createCommandEncoder({ label: "Colour Space Redraw" });
        recordDraws(encoder, targets, measure);
        finish(encoder, measure, start);
    }

    // Allocated on the first readback.
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
        outputSpace.destroy();
        timer.destroy();
        camera.destroy();
        xyBins.destroy();
        volBins.destroy();
        stats.destroy();
        scale.destroy();
        staging?.destroy();
    }

    // Avoid zero-filled uniforms before the panel's first update.
    setOutputSpace(gpu.displaySpace, null);
    setVolumeView({ yaw: 0.6, pitch: 0.42, aspect: 1 });

    return {
        setSourceSpace,
        setOverlays,
        setOutputSpace,
        setToneMap,
        setCurves,
        setVolumeView,
        render,
        redraw,
        readStats,
        destroy,
    };
}
