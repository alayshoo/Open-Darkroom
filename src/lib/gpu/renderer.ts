// src/lib/gpu/renderer.ts

import { defaultSlidersRGB, type Sliders } from "../types/imgParameters";
import {
    SHARP_FORMAT,
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
    createSharpHorizontalStage,
    createSharpVerticalStage,
    createSourceSampler,
} from "./pipelines/chainStages";
import { sharpnessIsVisible } from "./sharpnessVisibility";


export function createRenderer(gpu: GPUSession, canvas: GPUCanvasLink): Renderer {

    // Create pipelines at startup
    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);

    const develop: RenderStage = createDevelopStage(gpu);
    const sharpH: RenderStage = createSharpHorizontalStage(gpu);
    const sharpV: RenderStage = createSharpVerticalStage(gpu);
    const composite: RenderStage = createCompositeStage(gpu);
    const colorSpaceEncode: RenderStage = createEncodeStage(gpu);

    const sampler = createSourceSampler(gpu);

    // The sliders and the view are forwarded straight to calcParams, but they are
    // also kept here: whether the blur passes are worth recording is a decision
    // made on the CPU, and these are what it needs.
    let currentSliders: Sliders = defaultSlidersRGB;
    let currentRenderScale = 1;

    // Frame scheduling. A submit costs whatever the chain costs, so issuing them
    // faster than the GPU drains them backs the presentation queue up until
    // acquiring the next canvas texture blocks the calling thread — which stalls
    // input handling and the DOM alongside the preview.
    let dirty = false;
    let inFlight = false;
    let frameHandle: number | null = null;
    let destroyed = false;

    // Bind groups connect actual resources to the layout slots, and the
    // intermediate textures are sized to the image. Neither exists until an
    // image is loaded, so both start null.
    //
    // The views are held alongside the textures rather than made per frame: they
    // only change when the image does, and a slider drag would otherwise build
    // two of them on every frame for nothing.
    let developTexture: GPUTexture | null = null;
    let sharpHTexture: GPUTexture | null = null;
    let detailTexture: GPUTexture | null = null;
    let compositeTexture: GPUTexture | null = null;
    let developView: GPUTextureView | null = null;
    let sharpHView: GPUTextureView | null = null;
    let detailView: GPUTextureView | null = null;
    let compositeView: GPUTextureView | null = null;
    let developBindGroup: GPUBindGroup | null = null;
    let sharpHBindGroup: GPUBindGroup | null = null;
    let sharpVBindGroup: GPUBindGroup | null = null;
    let compositeBindGroup: GPUBindGroup | null = null;
    let encodeBindGroup: GPUBindGroup | null = null;
    let currentImage: GPUImage | null = null;

    function createIntermediate(
        label: string,
        image: GPUImage,
        format: GPUTextureFormat,
    ): GPUTexture {
        return gpu.device.createTexture({
            label,
            size: { width: image.width, height: image.height },
            format,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
        });
    }

    function loadImage(image: GPUImage) {
        currentImage = image;

        // The intermediates are sized to the image, so a new image needs new
        // ones — and new bind groups pointing at them.
        developTexture?.destroy();
        sharpHTexture?.destroy();
        detailTexture?.destroy();
        compositeTexture?.destroy();
        developTexture = createIntermediate("Developed Image", image, WORKING_FORMAT);
        sharpHTexture = createIntermediate("Sharp Blur H", image, SHARP_FORMAT);
        detailTexture = createIntermediate("Detail Bands", image, SHARP_FORMAT);
        compositeTexture = createIntermediate("Composited Image", image, WORKING_FORMAT);
        developView = developTexture.createView();
        sharpHView = sharpHTexture.createView();
        detailView = detailTexture.createView();
        compositeView = compositeTexture.createView();

        developBindGroup = createStageBindGroup(gpu, develop, [
            { binding: 0, resource: image.texture.createView() },
            { binding: 1, resource: sampler },
            { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
        ]);

        // The horizontal pass never reads binding 1; it is the same developed
        // texture it is already blurring, so the layout can stay shared.
        sharpHBindGroup = createStageBindGroup(gpu, sharpH, [
            { binding: 0, resource: developView },
            { binding: 1, resource: developView },
            { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
        ]);

        sharpVBindGroup = createStageBindGroup(gpu, sharpV, [
            { binding: 0, resource: sharpHView },
            { binding: 1, resource: developView },
            { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
        ]);

        compositeBindGroup = createStageBindGroup(gpu, composite, [
            { binding: 0, resource: developView },
            { binding: 1, resource: detailView },
            { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
        ]);

        encodeBindGroup = createStageBindGroup(gpu, colorSpaceEncode, [
            { binding: 0, resource: compositeView },
        ]);
    }

    function setSliders(sliders: Sliders) {
        currentSliders = sliders;
        calcParams.updateSliders(sliders);
    }

    // Where this render sits relative to the full-resolution image: how many
    // rendered pixels there are per image pixel, and how large the image is.
    // Clarity's radius is a fraction of the frame, so it needs both.
    function setView(renderScale: number, fullWidth: number, fullHeight: number) {
        currentRenderScale = renderScale;
        calcParams.updateView(renderScale, fullWidth, fullHeight);
    }

    // Ask for a frame. Repeated calls between two frames collapse into one, and
    // nothing is submitted while the previous frame is still outstanding, so the
    // preview redraws at whatever rate the GPU can actually sustain while the
    // caller stays free to keep handling input.
    function requestRender() {
        dirty = true;
        scheduleFrame();
    }

    function scheduleFrame() {
        if (destroyed || inFlight || frameHandle !== null) return;
        frameHandle = requestAnimationFrame(onFrame);
    }

    function onFrame() {
        frameHandle = null;
        if (destroyed || !dirty) return;
        // The request outlives a frame that had nothing to draw yet, so an image
        // arriving later still gets one.
        if (drawFrame()) dirty = false;
    }

    function onWorkDone() {
        inFlight = false;
        if (dirty) scheduleFrame();
    }

    function drawFrame(): boolean {
        if (!currentImage || !developView || !compositeView) return false;
        if (!sharpHView || !detailView) return false;
        if (!developBindGroup || !compositeBindGroup || !encodeBindGroup) return false;
        if (!sharpHBindGroup || !sharpVBindGroup) return false;

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
        recordFullScreenPass(encoder, develop, developView, developBindGroup);

        // The two halves of the separable blur, skipped whenever the effect
        // cannot reach the screen. Skipping is only ever a saving: calcParams
        // zeroes the amount over the same range, so composite is a no-op and the
        // stale bands in the detail texture go nowhere.
        if (sharpnessIsVisible(currentSliders, currentRenderScale)) {
            recordFullScreenPass(encoder, sharpH, sharpHView, sharpHBindGroup);
            recordFullScreenPass(encoder, sharpV, detailView, sharpVBindGroup);
        }

        recordFullScreenPass(encoder, composite, compositeView, compositeBindGroup);
        recordFullScreenPass(encoder, colorSpaceEncode, targetView, encodeBindGroup, {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1,
        });

        // Step 5: Submit, and hold the next frame until this one has drained.
        gpu.device.queue.submit([encoder.finish()]);

        inFlight = true;
        gpu.device.queue.onSubmittedWorkDone().then(onWorkDone);

        return true;
    }

    // Releases what the renderer allocated. The source texture is not included:
    // it is created by the caller and handed in, and the caller destroys it —
    // both when swapping images and on teardown.
    function destroy() {
        destroyed = true;
        dirty = false;
        if (frameHandle !== null) cancelAnimationFrame(frameHandle);
        frameHandle = null;

        calcParams.destroy();
        developTexture?.destroy();
        sharpHTexture?.destroy();
        detailTexture?.destroy();
        compositeTexture?.destroy();
    }

    // Initialize with default sliders
    setSliders( defaultSlidersRGB );

    return { loadImage, setSliders, setView, requestRender, destroy };
}
