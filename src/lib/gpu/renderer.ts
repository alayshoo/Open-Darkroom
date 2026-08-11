// src/lib/gpu/renderer.ts

import { defaultSlidersRGB, type Sliders } from "../types/imgParameters";
import {
    WORKING_FORMAT,
    type GPUSession,
    type GPUImage,
    type Renderer,
    type GPUCanvasLink,
    type RenderStage,
} from "$lib/types/gpuTypes";
import { createCalcParamsPipeline, type CalcParamsPipeline } from "./pipelines/calcParamsPipeline";
import {
    createStageBindGroup,
    recordFullScreenPass,
} from "./pipelines/renderStagePipeline";
import {
    createCompositeStage,
    createDevelopStage,
    createEncodeStage,
    createSourceSampler,
} from "./pipelines/chainStages";


export function createRenderer(gpu: GPUSession, canvas: GPUCanvasLink): Renderer {

    // Create pipelines at startup
    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);

    const develop: RenderStage = createDevelopStage(gpu);
    const composite: RenderStage = createCompositeStage(gpu);
    const colorSpaceEncode: RenderStage = createEncodeStage(gpu);

    const sampler = createSourceSampler(gpu);

    // Bind groups connect actual resources to the layout slots, and the
    // intermediate textures are sized to the image. Neither exists until an
    // image is loaded, so both start null.
    let developTexture: GPUTexture | null = null;
    let compositeTexture: GPUTexture | null = null;
    let developBindGroup: GPUBindGroup | null = null;
    let compositeBindGroup: GPUBindGroup | null = null;
    let encodeBindGroup: GPUBindGroup | null = null;
    let currentImage: GPUImage | null = null;

    function createIntermediate(label: string, image: GPUImage): GPUTexture {
        return gpu.device.createTexture({
            label,
            size: { width: image.width, height: image.height },
            format: WORKING_FORMAT,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
        });
    }

    function loadImage(image: GPUImage) {
        currentImage = image;

        // The intermediates are sized to the image, so a new image needs new
        // ones — and new bind groups pointing at them.
        developTexture?.destroy();
        compositeTexture?.destroy();
        developTexture = createIntermediate("Developed Image", image);
        compositeTexture = createIntermediate("Composited Image", image);

        developBindGroup = createStageBindGroup(gpu, develop, [
            { binding: 0, resource: image.texture.createView() },
            { binding: 1, resource: sampler },
            { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
        ]);

        compositeBindGroup = createStageBindGroup(gpu, composite, [
            { binding: 0, resource: developTexture.createView() },
        ]);

        encodeBindGroup = createStageBindGroup(gpu, colorSpaceEncode, [
            { binding: 0, resource: compositeTexture.createView() },
        ]);
    }

    function setSliders(sliders: Sliders) {
        calcParams.updateSliders(sliders);
    }

    // Rendered pixels per full-resolution image pixel — 1 when the surface is
    // the full image, below 1 for a downscaled preview.
    function setRenderScale(renderScale: number) {
        calcParams.updateView(renderScale);
    }

    function render() {
        if (!currentImage || !developTexture || !compositeTexture) return;
        if (!developBindGroup || !compositeBindGroup || !encodeBindGroup) return;

        // Step 1: Get the current canvas texture to render into.
        // This changes every frame (it's a swapchain texture).
        const targetTexture = canvas.canvasConfig.getCurrentTexture();
        const targetView = targetTexture.createView();

        // Step 2: Create a command encoder — this records GPU commands.
        // Nothing actually executes until we submit the command buffer.
        const encoder = gpu.device.createCommandEncoder({
            label: "Render Frame",
        });

        // Step 3: Compute params from sliders (matrices calculated on GPU).
        // This compute pass writes the Params storage buffer.
        calcParams.recordCalcParams(encoder);

        // Step 4: The chain. Each stage reads the previous one's target, so they
        // cannot be collapsed: a fragment can only read a texture it is not
        // currently writing.
        recordFullScreenPass(encoder, develop, developTexture.createView(), developBindGroup);
        recordFullScreenPass(encoder, composite, compositeTexture.createView(), compositeBindGroup);
        recordFullScreenPass(encoder, colorSpaceEncode, targetView, encodeBindGroup, {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1,
        });

        // Step 5: Submit
        gpu.device.queue.submit([encoder.finish()]);
    }

    function destroy() {
        calcParams.destroy();
        developTexture?.destroy();
        compositeTexture?.destroy();
        currentImage?.texture.destroy();
    }

    // Initialize with default sliders
    setSliders( defaultSlidersRGB );

    return { loadImage, setSliders, setRenderScale, render, destroy };
}
