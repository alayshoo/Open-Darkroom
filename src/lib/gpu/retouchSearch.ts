// src/lib/gpu/retouchSearch.ts
//
// Picks the offset a stroke takes its pixels from when none was picked, by
// scoring candidate offsets against the image as it currently is.

import type { GPUImage } from "$lib/types/gpuTypes";
import type { PlacedRetouch } from "$lib/types/retouch";
import { bestCandidate, ringSamples, sourceCandidates } from "$lib/utils/retouchSource";
import searchSource from "./shaders/retouchSource.wgsl?raw";

const SEARCH_UNIFORM_SIZE = 32;

const pipelines = new WeakMap<GPUDevice, GPUComputePipeline>();

function searchPipeline(device: GPUDevice): GPUComputePipeline {
    let pipeline = pipelines.get(device);
    if (pipeline) return pipeline;
    const storage = (binding: number, type: GPUBufferBindingType): GPUBindGroupLayoutEntry => ({
        binding,
        visibility: GPUShaderStage.COMPUTE,
        buffer: { type },
    });
    const layout = device.createBindGroupLayout({
        label: "Retouch Search",
        entries: [
            { binding: 0, visibility: GPUShaderStage.COMPUTE, texture: { sampleType: "unfilterable-float" } },
            storage(1, "uniform"),
            storage(2, "read-only-storage"),
            storage(3, "read-only-storage"),
            storage(4, "storage"),
        ],
    });
    pipeline = device.createComputePipeline({
        label: "Retouch Search",
        layout: device.createPipelineLayout({ bindGroupLayouts: [layout] }),
        compute: { module: device.createShaderModule({ label: "Retouch Search", code: searchSource }), entryPoint: "main" },
    });
    pipelines.set(device, pipeline);
    return pipeline;
}

/** The whole-texel offset for `stroke` on `image`, or null when nothing on the image fits. */
export async function findSource(device: GPUDevice, image: GPUImage, stroke: PlacedRetouch): Promise<[number, number] | null> {
    const { width, height } = image;
    const candidates = sourceCandidates(stroke, width, height);
    if (candidates.length === 0) return null;
    const samples = ringSamples(stroke, width, height);
    if (samples.length === 0) return candidates[0];

    const pipeline = searchPipeline(device);
    const buffer = (data: Float32Array | ArrayBuffer, usage: number) => {
        const b = device.createBuffer({ size: Math.max(16, data.byteLength), usage: usage | GPUBufferUsage.COPY_DST });
        device.queue.writeBuffer(b, 0, data);
        return b;
    };

    const uniform = new ArrayBuffer(SEARCH_UNIFORM_SIZE);
    new Uint32Array(uniform, 0, 3).set([samples.length, candidates.length, stroke.mode === "heal" ? 1 : 0]);
    new Float32Array(uniform, 16, 2).set([width, height]);

    const uniformBuffer = buffer(uniform, GPUBufferUsage.UNIFORM);
    const sampleBuffer = buffer(new Float32Array(samples.flat()), GPUBufferUsage.STORAGE);
    const candidateBuffer = buffer(new Float32Array(candidates.flat()), GPUBufferUsage.STORAGE);
    const scoreBytes = candidates.length * 4;
    const scores = device.createBuffer({ size: scoreBytes, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
    const readback = device.createBuffer({ size: scoreBytes, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });

    const bindGroup = device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [
            { binding: 0, resource: image.texture.createView() },
            { binding: 1, resource: { buffer: uniformBuffer } },
            { binding: 2, resource: { buffer: sampleBuffer } },
            { binding: 3, resource: { buffer: candidateBuffer } },
            { binding: 4, resource: { buffer: scores } },
        ],
    });

    const encoder = device.createCommandEncoder({ label: "Retouch Search" });
    const pass = encoder.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.dispatchWorkgroups(Math.ceil(candidates.length / 64));
    pass.end();
    encoder.copyBufferToBuffer(scores, 0, readback, 0, scoreBytes);
    device.queue.submit([encoder.finish()]);

    try {
        await readback.mapAsync(GPUMapMode.READ);
        const result = new Float32Array(readback.getMappedRange().slice(0));
        readback.unmap();
        return bestCandidate(candidates, result, stroke.radius) ?? candidates[0];
    } finally {
        for (const b of [uniformBuffer, sampleBuffer, candidateBuffer, scores, readback]) b.destroy();
    }
}
