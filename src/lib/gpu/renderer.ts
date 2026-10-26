// src/lib/gpu/renderer.ts

import { defaultSlidersRGB, type Sliders } from "../types/imgParameters";
import type { ToneMap } from "../types/toneMap";
import {
    SHARP_FORMAT,
    VIEW_FORMAT,
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
    type FullScreenPassOptions,
} from "./pipelines/renderStagePipeline";
import {
    createCompositeStage,
    createDevelopStage,
    createEncodeStage,
    createSharpHorizontalStage,
    createSharpVerticalStage,
    createSourceSampler,
    createViewSampleStage,
    createViewSampler,
} from "./pipelines/chainStages";
import { sharpnessIsVisible, sharpnessReach } from "./sharpnessVisibility";
import { createFrameTimer } from "./pipelines/frameTimer";
import { debugStats } from "./debugStats.svelte";
import { createOutputSpaceBuffer } from "./outputSpaceBuffer";
import type { OutputSpaceId } from "./outputSpace";
import { CROP_VIEW_BUFFER_SIZE, EVERYWHERE, cropViewToFloats } from "$lib/types/crop";
import {
    PAN_MARGIN,
    canvasToChain,
    chainToSource,
    contains,
    imageInChain,
    neededBox,
    planRegion,
    standsIn,
    validBox,
    visibleRegion,
    type Box,
    type ChainRegion,
    type ViewFrame,
    type Visible,
} from "$lib/utils/viewport";
import type { Affine } from "$lib/types/crop";


// Wherever the image is not drawn the canvas is left clear, so the backdrop
// behind it shows through.
const CLEAR: GPUColor = { r: 0, g: 0, b: 0, a: 0 };

// The chain's textures grow in steps of this many pixels, so a region that
// changes size a little from one render to the next does not reallocate.
const TEXTURE_STEP = 256;

// How long the zoom has to hold still before a region rendered at another
// scale is replaced by one at the current scale.
const SETTLE_MS = 120;

// The `ViewSample` uniform in viewSample.wgsl.
const VIEW_SAMPLE_BUFFER_SIZE = 32;

const affineToFloats = (a: Affine) => new Float32Array([...a.origin, ...a.axisX, ...a.axisY, 0, 0]);

export function createRenderer(gpu: GPUSession, canvas: GPUCanvasLink): Renderer {

    // Create pipelines at startup
    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);

    const viewSample: RenderStage = createViewSampleStage(gpu);
    const develop: RenderStage = createDevelopStage(gpu);
    const sharpH: RenderStage = createSharpHorizontalStage(gpu);
    const sharpV: RenderStage = createSharpVerticalStage(gpu);
    const composite: RenderStage = createCompositeStage(gpu);
    const colorSpaceEncode: RenderStage = createEncodeStage(gpu);

    const sampler = createSourceSampler(gpu);
    const viewSampler = createViewSampler(gpu);

    // Where the chain is rendering to. The buffer outlives every image, so the
    // destination can be set before one is open and survives a reload.
    const outputSpace = createOutputSpaceBuffer(gpu);

    const cropBuffer = gpu.device.createBuffer({
        label: "Crop View Uniform Buffer",
        size: CROP_VIEW_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    const viewSampleBuffer = gpu.device.createBuffer({
        label: "View Sample Uniform Buffer",
        size: VIEW_SAMPLE_BUFFER_SIZE,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    // The sliders are forwarded straight to calcParams, but they are also kept
    // here: whether the blur passes are worth recording, and how far past the
    // screen they need pixels, are decisions made on the CPU.
    let currentSliders: Sliders = defaultSlidersRGB;

    let source: GPUImage | null = null;
    let frame: ViewFrame | null = null;
    // What the chain's textures hold, and whether the adjustments have moved
    // on since it was rendered. While they have not, a frame that only moves
    // the view re-runs the encode stage alone.
    let region: ChainRegion | null = null;
    let stale = true;
    let scaleChangedAt = 0;
    let settleTimer: ReturnType<typeof setTimeout> | null = null;

    // Frame scheduling. A submit costs whatever the chain costs, so issuing them
    // faster than the GPU drains them backs the presentation queue up until
    // acquiring the next canvas texture blocks the calling thread — which stalls
    // input handling and the DOM alongside the preview.
    let dirty = false;
    let inFlight = false;
    let frameHandle: number | null = null;
    let destroyed = false;

    const frameTimer = createFrameTimer(gpu);
    debugStats.supportsPassTimings = frameTimer.available;
    let lastFrameStart = 0;

    // The chain's textures are sized to the region it renders, and the bind
    // groups point at them, so neither exists until a region has been planned.
    // The blur textures are only made once sharpening is first visible; until
    // then composite reads the developed texture in their place, which it
    // never looks at with the amounts at zero.
    let capacity = { width: 0, height: 0 };
    let viewTexture: GPUTexture | null = null;
    let developTexture: GPUTexture | null = null;
    let compositeTexture: GPUTexture | null = null;
    let sharpHTexture: GPUTexture | null = null;
    let detailTexture: GPUTexture | null = null;
    let viewSampleBindGroup: GPUBindGroup | null = null;
    let developBindGroup: GPUBindGroup | null = null;
    let sharpHBindGroup: GPUBindGroup | null = null;
    let sharpVBindGroup: GPUBindGroup | null = null;
    let compositeBindGroup: GPUBindGroup | null = null;
    let encodeBindGroup: GPUBindGroup | null = null;
    let viewView: GPUTextureView | null = null;
    let developView: GPUTextureView | null = null;
    let compositeView: GPUTextureView | null = null;
    let sharpHView: GPUTextureView | null = null;
    let detailView: GPUTextureView | null = null;

    function createIntermediate(label: string, format: GPUTextureFormat): GPUTexture {
        return gpu.device.createTexture({
            label,
            size: capacity,
            format,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
        });
    }

    function destroyIntermediates() {
        for (const t of [viewTexture, developTexture, compositeTexture, sharpHTexture, detailTexture]) {
            t?.destroy();
        }
        sharpHTexture = detailTexture = null;
        sharpHView = detailView = null;
    }

    // Sizes the chain to hold `width` × `height` render pixels, reallocating
    // only when they are outgrown or far too large, and adds the blur textures
    // the first time sharpening needs them.
    function fitTextures(width: number, height: number, sharpening: boolean) {
        const limit = gpu.device.limits.maxTextureDimension2D;
        const step = (n: number) => Math.min(limit, Math.ceil(Math.max(n, 1) / TEXTURE_STEP) * TEXTURE_STEP);
        const next = { width: step(width), height: step(height) };
        const resized =
            next.width > capacity.width ||
            next.height > capacity.height ||
            next.width * next.height * 2 < capacity.width * capacity.height;

        if (resized) {
            destroyIntermediates();
            capacity = next;
            viewTexture = createIntermediate("View Sample", VIEW_FORMAT);
            developTexture = createIntermediate("Developed Image", WORKING_FORMAT);
            compositeTexture = createIntermediate("Composited Image", WORKING_FORMAT);
            viewView = viewTexture.createView();
            developView = developTexture.createView();
            compositeView = compositeTexture.createView();
        }
        const addSharp = sharpening && !sharpHTexture;
        if (addSharp) {
            sharpHTexture = createIntermediate("Sharp Blur H", SHARP_FORMAT);
            detailTexture = createIntermediate("Detail Bands", SHARP_FORMAT);
            sharpHView = sharpHTexture.createView();
            detailView = detailTexture.createView();
        }
        if (resized || addSharp) bindIntermediates();
    }

    function bindIntermediates() {
        developBindGroup = createStageBindGroup(gpu, develop, [
            { binding: 0, resource: viewView! },
            { binding: 1, resource: sampler },
            { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
        ]);

        if (sharpHView && detailView) {
            // The horizontal pass never reads binding 1; it is the same developed
            // texture it is already blurring, so the layout can stay shared.
            sharpHBindGroup = createStageBindGroup(gpu, sharpH, [
                { binding: 0, resource: developView! },
                { binding: 1, resource: developView! },
                { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
            ]);
            sharpVBindGroup = createStageBindGroup(gpu, sharpV, [
                { binding: 0, resource: sharpHView },
                { binding: 1, resource: developView! },
                { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
            ]);
        }

        compositeBindGroup = createStageBindGroup(gpu, composite, [
            { binding: 0, resource: developView! },
            { binding: 1, resource: detailView ?? developView! },
            { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
        ]);

        encodeBindGroup = createStageBindGroup(gpu, colorSpaceEncode, [
            { binding: 0, resource: compositeView! },
            { binding: 1, resource: { buffer: outputSpace.buffer } },
            { binding: 2, resource: { buffer: cropBuffer } },
        ]);
    }

    function bindSource(image: GPUImage) {
        source = image;
        viewSampleBindGroup = createStageBindGroup(gpu, viewSample, [
            { binding: 0, resource: image.texture.createView() },
            { binding: 1, resource: viewSampler },
            { binding: 2, resource: { buffer: viewSampleBuffer } },
        ]);
        stale = true;
    }

    function loadImage(image: GPUImage, srcToWorking: Float32Array) {
        // The source's primaries, for calcParams to compose into the matrix
        // that ends the darkroom block.
        calcParams.updateSourceSpace(srcToWorking);
        bindSource(image);
        region = null;
    }

    function setSource(image: GPUImage) {
        bindSource(image);
    }

    function setFrame(next: ViewFrame) {
        if (!frame || next.scale !== frame.scale) scaleChangedAt = performance.now();
        frame = next;
    }

    // The destination the preview is encoded for, and the export space it is
    // being proofed against. Does not itself request a frame: the caller owns
    // when the preview redraws, as it does for the sliders. Only the encode
    // stage reads it, so the chain's region stays good.
    function setOutputSpace(destination: OutputSpaceId, proof: OutputSpaceId | null) {
        outputSpace.set(destination, proof);
    }

    function setToneMap(toneMap: ToneMap) {
        calcParams.updateToneMap(toneMap);
        stale = true;
    }

    function setCurves(lut: Float32Array | null) {
        calcParams.updateCurves(lut);
        stale = true;
    }

    function setSliders(sliders: Sliders) {
        currentSliders = sliders;
        calcParams.updateSliders(sliders);
        stale = true;
    }

    // The region the next frame has to render, or null if the one the chain
    // holds can still be drawn from. One rendered at another scale stands in
    // while a zoom is moving, and is replaced once it settles.
    function planNext(view: ViewFrame, image: GPUImage, visible: Visible): ChainRegion | null {
        const { fullWidth, fullHeight } = view;
        // Never finer than the source, nor than 1:1: past it the encode stage
        // enlarges.
        const scale = Math.min(view.scale, 1, image.width / fullWidth);

        if (region && !stale) {
            const covered = contains(validBox(region), neededBox(visible.image, region.scale));
            if (covered && region.scale === scale) return null;
            if (covered && standsIn(region, scale)) {
                const wait = SETTLE_MS - (performance.now() - scaleChangedAt);
                if (wait > 0) {
                    settleTimer ??= setTimeout(() => {
                        settleTimer = null;
                        requestRender();
                    }, wait);
                    return null;
                }
            }
        }

        const needed = neededBox(visible.image, scale);
        const whole = neededBox([0, 0, fullWidth, fullHeight], scale);
        const margin = contains(needed, whole) ? 0 : PAN_MARGIN;
        const apron = sharpnessReach(currentSliders, scale, fullWidth, fullHeight);
        const limit = gpu.device.limits.maxTextureDimension2D;
        return planRegion(needed, scale, apron, margin, fullWidth, fullHeight, limit);
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

    // Whether the frame currently being recorded is measured. Held here rather
    // than passed down so the helpers below can be built once instead of per
    // frame — an unmeasured frame must cost exactly what it did before the
    // overlay existed, and a closure rebuilt 60 times a second is not that.
    let measuring = false;

    function passOptions(label: string, scissor: [number, number, number, number]): FullScreenPassOptions {
        return measuring
            ? { scissor, timestampWrites: frameTimer.passWrites(label) }
            : { scissor };
    }

    function onWorkDone() {
        inFlight = false;
        if (dirty) scheduleFrame();
    }

    async function recordSample(
        intervalMs: number,
        acquireMs: number,
        recordMs: number,
        submitEnd: number,
    ) {
        const turnaroundMs = performance.now() - submitEnd;
        const passes = await frameTimer.read();
        if (destroyed) return;
        debugStats.record({ intervalMs, acquireMs, recordMs, turnaroundMs, passes });
    }

    function recordChain(encoder: GPUCommandEncoder, view: ViewFrame, image: GPUImage, next: ChainRegion) {
        const { fullWidth, fullHeight } = view;
        const width = next.box[2] - next.box[0];
        const height = next.box[3] - next.box[1];
        const sharpening = sharpnessIsVisible(currentSliders, next.scale);
        fitTextures(width, height, sharpening);

        gpu.device.queue.writeBuffer(
            viewSampleBuffer,
            0,
            affineToFloats(chainToSource(next, image.width, image.height, fullWidth, fullHeight)),
        );
        // Radii are full-resolution measurements, so the blurs need to know
        // how far this render is from full resolution.
        calcParams.updateView(next.scale, fullWidth, fullHeight);
        calcParams.recordCalcParams(
            encoder,
            measuring ? frameTimer.passWrites("params") : undefined,
        );

        // Each stage reads the previous one's target, so they cannot be
        // collapsed: a fragment can only read a texture it is not currently
        // writing.
        const scissor: [number, number, number, number] = [0, 0, width, height];
        recordFullScreenPass(encoder, viewSample, viewView!, viewSampleBindGroup!, passOptions("view", scissor));
        recordFullScreenPass(encoder, develop, developView!, developBindGroup!, passOptions("develop", scissor));

        // The two halves of the separable blur, skipped whenever the effect
        // cannot reach the screen. Skipping is only ever a saving: calcParams
        // zeroes the amount over the same range, so composite is a no-op.
        if (sharpening) {
            recordFullScreenPass(encoder, sharpH, sharpHView!, sharpHBindGroup!, passOptions("sharp H", scissor));
            recordFullScreenPass(encoder, sharpV, detailView!, sharpVBindGroup!, passOptions("sharp V", scissor));
        }

        recordFullScreenPass(encoder, composite, compositeView!, compositeBindGroup!, passOptions("composite", scissor));

        region = next;
        stale = false;
    }

    function recordEncode(encoder: GPUCommandEncoder, target: GPUTexture, view: ViewFrame, visible: Visible | null) {
        if (!visible || !region || !encodeBindGroup) {
            const pass = encoder.beginRenderPass({
                label: "Clear Pass",
                colorAttachments: [{ view: target.createView(), clearValue: CLEAR, loadOp: "clear", storeOp: "store" }],
            });
            pass.end();
            return;
        }

        gpu.device.queue.writeBuffer(
            cropBuffer,
            0,
            cropViewToFloats(
                { ...canvasToChain(view, region), width: view.canvasWidth, height: view.canvasHeight },
                view.keep,
                imageInChain(region, view.fullWidth, view.fullHeight),
                view.clip ?? EVERYWHERE,
            ),
        );

        const drawn = pixelBox(visible.canvas, target.width, target.height);
        recordFullScreenPass(encoder, colorSpaceEncode, target.createView(), encodeBindGroup, {
            ...passOptions("encode", drawn),
            clearValue: CLEAR,
        });
    }

    function drawFrame(): boolean {
        if (!source || !frame || !viewSampleBindGroup) return false;
        const view = frame;
        const image = source;

        // Measuring is opt-in: timestamp queries are not free, and neither are
        // the clocks around them, so both follow the overlay.
        measuring = debugStats.enabled;
        const frameStart = measuring ? performance.now() : 0;
        const intervalMs = measuring && lastFrameStart ? frameStart - lastFrameStart : 0;
        if (measuring) {
            lastFrameStart = frameStart;
            frameTimer.beginFrame();
        }

        // Step 1: Get the current canvas texture to render into.
        // This changes every frame (it's a swapchain texture).
        const targetTexture = canvas.canvasConfig.getCurrentTexture();
        const acquireEnd = measuring ? performance.now() : 0;

        // Step 2: Create a command encoder — this records GPU commands.
        // Nothing actually executes until we submit the command buffer.
        const encoder = gpu.device.createCommandEncoder({
            label: "Render Frame",
        });

        // Step 3: The chain, when the region it holds no longer serves, then
        // the output transform onto the canvas.
        const visible = visibleRegion(view);
        const next = visible ? planNext(view, image, visible) : null;
        if (next) recordChain(encoder, view, image, next);
        recordEncode(encoder, targetTexture, view, visible);

        if (measuring) frameTimer.resolve(encoder);

        // Step 4: Submit, and hold the next frame until this one has drained.
        gpu.device.queue.submit([encoder.finish()]);

        inFlight = true;
        const drained = gpu.device.queue.onSubmittedWorkDone();

        // The unmeasured path ends here, on a callback built once at startup.
        if (!measuring) {
            drained.then(onWorkDone);
            return true;
        }

        const submitEnd = performance.now();
        drained.then(() => {
            onWorkDone();
            void recordSample(
                intervalMs,
                acquireEnd - frameStart,
                submitEnd - acquireEnd,
                submitEnd,
            );
        });

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
        if (settleTimer !== null) clearTimeout(settleTimer);
        settleTimer = null;

        calcParams.destroy();
        outputSpace.destroy();
        cropBuffer.destroy();
        viewSampleBuffer.destroy();
        frameTimer.destroy();
        destroyIntermediates();
    }

    // Initialize with default sliders, and with the display as the destination
    // — which is what an unproofed preview renders for.
    setSliders( defaultSlidersRGB );
    setOutputSpace(gpu.displaySpace, null);

    return {
        loadImage, setSource, setSliders, setOutputSpace, setToneMap, setCurves, setFrame,
        requestRender, destroy,
    };
}

/** The whole pixels `box` touches, clamped to the target, as a scissor rect. */
function pixelBox(box: Box, width: number, height: number): [number, number, number, number] {
    const x0 = Math.max(0, Math.floor(box[0]));
    const y0 = Math.max(0, Math.floor(box[1]));
    const x1 = Math.min(width, Math.ceil(box[2]));
    const y1 = Math.min(height, Math.ceil(box[3]));
    return [x0, y0, Math.max(0, x1 - x0), Math.max(0, y1 - y0)];
}
