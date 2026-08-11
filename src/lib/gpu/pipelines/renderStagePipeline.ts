// src/lib/gpu/pipelines/renderStagePipeline.ts

import type { GPUSession, RenderStage } from "$lib/types/gpuTypes";

// Every stage of the render chain is the same shape: draw a full-screen quad,
// read some bound resources, write one target. They differ only in what they
// read, so that is the one thing passed in — develop takes the source texture, a
// sampler and the params buffer; the stages after it read the previous target.
//
// Bind groups are built by the caller, which is the only place that knows which
// resources a given frame is pointing at.
export function createRenderStage(
    gpu: GPUSession,
    options: {
        label: string;
        code: string;
        format: GPUTextureFormat;
        bindings: GPUBindGroupLayoutEntry[];
        // Defaults to fs_main. Named only where one file holds several stages,
        // as the two halves of the separable blur do.
        entryPoint?: string;
    },
): RenderStage {
    const { device } = gpu;

    const shaderModule = device.createShaderModule({
        label: `${options.label} Shader`,
        code: options.code,
    });

    const bindGroupLayout = device.createBindGroupLayout({
        label: `${options.label} Bind Group Layout`,
        entries: options.bindings,
    });

    const pipeline = device.createRenderPipeline({
        label: `${options.label} Pipeline`,
        layout: device.createPipelineLayout({ bindGroupLayouts: [bindGroupLayout] }),
        vertex: { module: shaderModule, entryPoint: "vs_main" },
        fragment: {
            module: shaderModule,
            entryPoint: options.entryPoint ?? "fs_main",
            targets: [{ format: options.format }],
        },
    });

    return { label: options.label, pipeline, bindGroupLayout };
}

// Bind a stage's resources for a frame.
export function createStageBindGroup(
    gpu: GPUSession,
    stage: RenderStage,
    entries: GPUBindGroupEntry[],
): GPUBindGroup {
    return gpu.device.createBindGroup({
        label: `${stage.label} Bind Group`,
        layout: stage.bindGroupLayout,
        entries,
    });
}

// Draw a full-screen quad from `stage` into `target`.
export function recordFullScreenPass(
    encoder: GPUCommandEncoder,
    stage: RenderStage,
    target: GPUTextureView,
    bindGroup: GPUBindGroup,
    clearValue: GPUColor = { r: 0, g: 0, b: 0, a: 1 },
) {
    const pass = encoder.beginRenderPass({
        label: `${stage.label} Pass`,
        colorAttachments: [
            { view: target, clearValue, loadOp: "clear", storeOp: "store" },
        ],
    });
    pass.setPipeline(stage.pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(6); // 6 vertices = full-screen quad (2 triangles)
    pass.end();
}

// A texture the chain hands between its stages: written by one pass, read one to
// one by the next. 32-bit float textures cannot be filtered, so these are always
// read with textureLoad.
export function intermediateBinding(binding: number): GPUBindGroupLayoutEntry {
    return {
        binding,
        visibility: GPUShaderStage.FRAGMENT,
        texture: { sampleType: "unfilterable-float" },
    };
}
