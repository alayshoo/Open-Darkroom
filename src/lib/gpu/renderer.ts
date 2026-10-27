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
    createDevelopSampler,
    createDevelopStage,
    createEncodeStage,
    createSharpHorizontalStage,
    createSharpVerticalStage,
    createViewSampleStage,
    createViewSampler,
} from "./pipelines/chainStages";
import { sharpnessIsVisible, sharpnessReach } from "./sharpnessVisibility";
import { createFrameTimer } from "./pipelines/frameTimer";
import { debugStats } from "./debugStats.svelte";
import { previewDebug } from "./previewDebug.svelte";
import { createOutputSpaceBuffer } from "./outputSpaceBuffer";
import type { OutputSpaceId } from "./outputSpace";
import { CROP_VIEW_BUFFER_SIZE, EVERYWHERE, cropViewToFloats } from "$lib/types/crop";
import {
    PAN_MARGIN,
    backdropScale,
    canvasToChain,
    chainToSource,
    contains,
    detailAction,
    imageInChain,
    intersect,
    neededBox,
    planRegion,
    showsDetail,
    validBox,
    visibleRegion,
    type Box,
    type ChainRegion,
    type ViewFrame,
} from "$lib/utils/viewport";
import type { Affine } from "$lib/types/crop";


// Wherever the image is not drawn the canvas is left clear, so the backdrop
// behind it shows through.
const CLEAR: GPUColor = { r: 0, g: 0, b: 0, a: 0 };

// The chain's textures grow in steps of this many pixels.
const TEXTURE_STEP = 256;

// The widest apron the blurs can ask for. Must match MAX_RADIUS in
// calcSharpTexture.wgsl.
const MAX_APRON = 180;

// How long the view has to hold still before a render that was put off runs.
const SETTLE_MS = 120;

// A sharp render that took less than this is redone even mid-gesture.
const CHEAP_MS = 12;

// How long after the adjustments change the backdrop catches up, once idle.
const BACKDROP_IDLE_MS = 300;

// The `ViewSample` uniform in viewSample.wgsl.
const VIEW_SAMPLE_BUFFER_SIZE = 32;

// The `ViewSample` uniform: the map, and whether it copies whole texels — one
// texel per pixel along the source's own axes, from a texel centre.
function viewSampleFloats(a: Affine): Float32Array {
    const exact =
        a.origin.every(Number.isInteger) && [...a.axisX, ...a.axisY].every((v) => v === 0 || Math.abs(v) === 1);
    return new Float32Array([...a.origin, ...a.axisX, ...a.axisY, exact ? 1 : 0, 0]);
}

const step = (n: number, limit: number) =>
    Math.min(limit, Math.ceil(Math.max(n, 1) / TEXTURE_STEP) * TEXTURE_STEP);

/** One rendered output of the chain, and how to draw it onto the canvas. */
interface Held {
    region: ChainRegion;
    texture: GPUTexture;
    encode: GPUBindGroup;
    crop: GPUBuffer;
}

export function createRenderer(gpu: GPUSession, canvas: GPUCanvasLink): Renderer {

    // Create pipelines at startup
    const calcParams: CalcParamsPipeline = createCalcParamsPipeline(gpu);

    const viewSample: RenderStage = createViewSampleStage(gpu);
    const develop: RenderStage = createDevelopStage(gpu);
    const sharpH: RenderStage = createSharpHorizontalStage(gpu);
    const sharpV: RenderStage = createSharpVerticalStage(gpu);
    const composite: RenderStage = createCompositeStage(gpu);
    const colorSpaceEncode: RenderStage = createEncodeStage(gpu);
    const colorSpaceEncodeOver: RenderStage = createEncodeStage(gpu, true);

    const sampler = createDevelopSampler(gpu);
    const viewSampler = createViewSampler(gpu);

    // Where the chain is rendering to. The buffer outlives every image, so the
    // destination can be set before one is open and survives a reload.
    const outputSpace = createOutputSpaceBuffer(gpu);

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

    // The sharp region, at the scale the view asks for, and the whole image at
    // whatever scale fits the chain, drawn wherever the sharp region does not
    // reach. Each goes stale when the adjustments move on.
    let detail: Held | null = null;
    let backdrop: Held | null = null;
    let detailStale = true;
    let backdropStale = true;

    // When the view last moved, and what the last sharp render cost: a cheap
    // one is redone mid-gesture, a costly one waits for the view to settle.
    let movedAt = 0;
    let lastDetailMs = 0;
    let settleTimer: ReturnType<typeof setTimeout> | null = null;
    let idleTimer: ReturnType<typeof setTimeout> | null = null;
    let counters = { detailRenders: 0, backdropRenders: 0, deferred: 0 };

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

    // Scratch for one run of the chain, sized from what the image covers on
    // the canvas, and from the whole canvas once a zoom starts, so a gesture
    // reallocates at most once. The blur textures wait until sharpening is first
    // visible; until then composite reads the developed texture in their place.
    let capacity = { width: 0, height: 0 };
    let canvasSize = { width: 0, height: 0 };
    let freshImage = true;
    let viewTexture: GPUTexture | null = null;
    let developTexture: GPUTexture | null = null;
    let sharpHTexture: GPUTexture | null = null;
    let bandsTexture: GPUTexture | null = null;
    let viewSampleBindGroup: GPUBindGroup | null = null;
    let developBindGroup: GPUBindGroup | null = null;
    let sharpHBindGroup: GPUBindGroup | null = null;
    let sharpVBindGroup: GPUBindGroup | null = null;
    let compositeBindGroup: GPUBindGroup | null = null;

    function createTexture(label: string, size: { width: number; height: number }, format: GPUTextureFormat) {
        return gpu.device.createTexture({
            label,
            size,
            format,
            usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING,
        });
    }

    function releaseHeld(held: Held | null) {
        held?.texture.destroy();
        held?.crop.destroy();
    }

    function destroyScratch() {
        for (const t of [viewTexture, developTexture, sharpHTexture, bandsTexture]) t?.destroy();
        viewTexture = developTexture = sharpHTexture = bandsTexture = null;
    }

    // Room for the canvas, the pan margin and the widest apron on every side.
    // Grows past that only for a region that needs it, such as a rotated crop.
    function fitScratch(width: number, height: number, sharpening: boolean) {
        const limit = gpu.device.limits.maxTextureDimension2D;
        const next = {
            width: step(Math.max(width, capacity.width), limit),
            height: step(Math.max(height, capacity.height), limit),
        };
        const resized = next.width !== capacity.width || next.height !== capacity.height;
        if (resized) {
            destroyScratch();
            capacity = next;
            viewTexture = createTexture("View Sample", capacity, VIEW_FORMAT);
            developTexture = createTexture("Developed Image", capacity, WORKING_FORMAT);
        }
        const addSharp = sharpening && !sharpHTexture;
        if (addSharp) {
            sharpHTexture = createTexture("Sharp Blur H", capacity, SHARP_FORMAT);
            bandsTexture = createTexture("Detail Bands", capacity, SHARP_FORMAT);
        }
        if (resized || addSharp) bindScratch();
    }

    function bindScratch() {
        const viewView = viewTexture!.createView();
        const developView = developTexture!.createView();
        developBindGroup = createStageBindGroup(gpu, develop, [
            { binding: 0, resource: viewView },
            { binding: 1, resource: sampler },
            { binding: 2, resource: { buffer: calcParams.paramsBuffer } },
        ]);

        if (sharpHTexture && bandsTexture) {
            // The horizontal pass never reads binding 1; it is the same developed
            // texture it is already blurring, so the layout can stay shared.
            sharpHBindGroup = createStageBindGroup(gpu, sharpH, [
                { binding: 0, resource: developView },
                { binding: 1, resource: developView },
                { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
            ]);
            sharpVBindGroup = createStageBindGroup(gpu, sharpV, [
                { binding: 0, resource: sharpHTexture.createView() },
                { binding: 1, resource: developView },
                { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
            ]);
        }

        compositeBindGroup = createStageBindGroup(gpu, composite, [
            { binding: 0, resource: developView },
            { binding: 1, resource: bandsTexture?.createView() ?? developView },
            { binding: 2, resource: { buffer: calcParams.sharpBuffer } },
        ]);
    }

    // An output texture for `region`, reusing `held`'s when it is large enough.
    function outputFor(held: Held | null, region: ChainRegion, label: string): Held {
        const width = region.box[2] - region.box[0];
        const height = region.box[3] - region.box[1];
        if (held && held.texture.width >= width && held.texture.height >= height) {
            return { ...held, region };
        }
        releaseHeld(held);
        const limit = gpu.device.limits.maxTextureDimension2D;
        const texture = createTexture(label, { width: step(width, limit), height: step(height, limit) }, WORKING_FORMAT);
        const crop = gpu.device.createBuffer({
            label: `${label} Crop View`,
            size: CROP_VIEW_BUFFER_SIZE,
            usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
        });
        const encode = createStageBindGroup(gpu, colorSpaceEncode, [
            { binding: 0, resource: texture.createView() },
            { binding: 1, resource: { buffer: outputSpace.buffer } },
            { binding: 2, resource: { buffer: crop } },
        ]);
        return { region, texture, encode, crop };
    }

    function bindSource(image: GPUImage) {
        source = image;
        viewSampleBindGroup = createStageBindGroup(gpu, viewSample, [
            { binding: 0, resource: image.texture.createView() },
            { binding: 1, resource: viewSampler },
            { binding: 2, resource: { buffer: viewSampleBuffer } },
        ]);
        detailStale = backdropStale = true;
    }

    function loadImage(image: GPUImage, srcToWorking: Float32Array) {
        // The source's primaries, for calcParams to compose into the matrix
        // that ends the darkroom block.
        calcParams.updateSourceSpace(srcToWorking);
        bindSource(image);
        releaseHeld(detail);
        releaseHeld(backdrop);
        detail = backdrop = null;
        freshImage = true;
    }

    function setSource(image: GPUImage) {
        bindSource(image);
    }

    function setFrame(next: ViewFrame) {
        const moved =
            !frame ||
            next.scale !== frame.scale ||
            next.toImage.origin.some((v, i) => v !== frame!.toImage.origin[i]) ||
            next.toImage.axisX.some((v, i) => v !== frame!.toImage.axisX[i]);
        if (moved) movedAt = performance.now();
        const resized = next.canvasWidth !== canvasSize.width || next.canvasHeight !== canvasSize.height;
        const zoomingIn = !resized && !freshImage && !!frame && next.scale > frame.scale;
        frame = next;

        if (resized) {
            canvasSize = { width: next.canvasWidth, height: next.canvasHeight };
            // The backdrop is sized from the canvas.
            backdropStale = true;
        }
        if (!resized && !zoomingIn && !freshImage) return;

        const reach = 2 * (PAN_MARGIN + MAX_APRON + 2);
        const shown = zoomingIn ? [0, 0, next.canvasWidth, next.canvasHeight] : visibleRegion(next)?.canvas;
        const width = Math.ceil(shown ? shown[2] - shown[0] : 0) + reach;
        const height = Math.ceil(shown ? shown[3] - shown[1] : 0) + reach;
        // Shrinks only once the scratch is well past what is needed, so
        // dragging a window edge does not reallocate on every frame.
        if (!zoomingIn && 2 * width * height < capacity.width * capacity.height) {
            capacity = { width: 0, height: 0 };
            releaseHeld(backdrop);
            backdrop = null;
            backdropStale = true;
        }
        fitScratch(width, height, !!sharpHTexture);
        freshImage = false;
    }

    function markStale() {
        detailStale = backdropStale = true;
    }

    // The destination the preview is encoded for, and the export space it is
    // being proofed against. Does not itself request a frame: the caller owns
    // when the preview redraws, as it does for the sliders. Only the encode
    // stage reads it, so what the chain holds stays good.
    function setOutputSpace(destination: OutputSpaceId, proof: OutputSpaceId | null) {
        outputSpace.set(destination, proof);
    }

    function setToneMap(toneMap: ToneMap) {
        calcParams.updateToneMap(toneMap);
        markStale();
    }

    function setCurves(lut: Float32Array | null) {
        calcParams.updateCurves(lut);
        markStale();
    }

    function setSliders(sliders: Sliders) {
        currentSliders = sliders;
        calcParams.updateSliders(sliders);
        markStale();
    }

    function later(timer: ReturnType<typeof setTimeout> | null, ms: number) {
        if (timer !== null) clearTimeout(timer);
        return setTimeout(() => requestRender(), Math.max(ms, 0));
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

    // One run of the chain over `region`, into `output`. Each run writes the
    // view uniforms, so two runs need two submits to keep their own.
    function recordChain(encoder: GPUCommandEncoder, view: ViewFrame, image: GPUImage, region: ChainRegion, output: GPUTexture, timed: boolean) {
        const { fullWidth, fullHeight } = view;
        const width = region.box[2] - region.box[0];
        const height = region.box[3] - region.box[1];
        const sharpening = sharpnessIsVisible(currentSliders, region.scale);
        fitScratch(width, height, sharpening);

        gpu.device.queue.writeBuffer(
            viewSampleBuffer,
            0,
            viewSampleFloats(chainToSource(region, image.width, image.height, fullWidth, fullHeight)),
        );
        // Radii are full-resolution measurements, so the blurs need to know
        // how far this render is from full resolution.
        calcParams.updateView(region.scale, fullWidth, fullHeight);
        const options = (label: string, scissor: [number, number, number, number]) =>
            timed ? passOptions(label, scissor) : { scissor };
        calcParams.recordCalcParams(encoder, timed && measuring ? frameTimer.passWrites("params") : undefined);

        // Each stage reads the previous one's target, so they cannot be
        // collapsed: a fragment can only read a texture it is not currently
        // writing.
        const scissor: [number, number, number, number] = [0, 0, width, height];
        recordFullScreenPass(encoder, viewSample, viewTexture!.createView(), viewSampleBindGroup!, options("view", scissor));
        recordFullScreenPass(encoder, develop, developTexture!.createView(), developBindGroup!, options("develop", scissor));

        // The two halves of the separable blur, skipped whenever the effect
        // cannot reach the screen. Skipping is only ever a saving: calcParams
        // zeroes the amount over the same range, so composite is a no-op.
        if (sharpening) {
            recordFullScreenPass(encoder, sharpH, sharpHTexture!.createView(), sharpHBindGroup!, options("sharp H", scissor));
            recordFullScreenPass(encoder, sharpV, bandsTexture!.createView(), sharpVBindGroup!, options("sharp V", scissor));
        }

        recordFullScreenPass(encoder, composite, output.createView(), compositeBindGroup!, options("composite", scissor));
    }

    // The whole image at about half the canvas's resolution: it only fills in
    // until the sharp region catches up.
    function renderBackdrop(view: ViewFrame, image: GPUImage) {
        const { fullWidth, fullHeight } = view;
        const width = Math.ceil(view.canvasWidth / 2) + 2 * MAX_APRON + 4;
        const height = Math.ceil(view.canvasHeight / 2) + 2 * MAX_APRON + 4;
        const fit = backdropScale(fullWidth, fullHeight, image.width / fullWidth, width, height, MAX_APRON);
        const apron = sharpnessReach(currentSliders, fit, fullWidth, fullHeight);
        const region = planRegion(neededBox([0, 0, fullWidth, fullHeight], fit), fit, apron, 0, fullWidth, fullHeight, Math.max(width, height));
        backdrop = outputFor(backdrop, region, "Backdrop");
        const encoder = gpu.device.createCommandEncoder({ label: "Render Backdrop" });
        recordChain(encoder, view, image, region, backdrop.texture, false);
        gpu.device.queue.submit([encoder.finish()]);
        backdropStale = false;
        counters.backdropRenders++;
    }

    function writeCrop(held: Held, view: ViewFrame, image: Box) {
        gpu.device.queue.writeBuffer(
            held.crop,
            0,
            cropViewToFloats(
                { ...canvasToChain(view, held.region), width: view.canvasWidth, height: view.canvasHeight },
                view.keep,
                image,
                view.clip ?? EVERYWHERE,
            ),
        );
    }

    function drawFrame(): boolean {
        if (!source || !frame || !viewSampleBindGroup || !developBindGroup) return false;
        const view = frame;
        const image = source;
        const now = performance.now();

        // Measuring is opt-in: timestamp queries are not free, and neither are
        // the clocks around them, so both follow the overlay.
        measuring = debugStats.enabled;
        const frameStart = measuring ? now : 0;
        const intervalMs = measuring && lastFrameStart ? frameStart - lastFrameStart : 0;
        if (measuring) {
            lastFrameStart = frameStart;
            frameTimer.beginFrame();
        }

        const { fullWidth, fullHeight } = view;
        const visible = visibleRegion(view);
        // Never finer than the source, nor than 1:1: past it the encode stage
        // enlarges.
        const scale = Math.min(view.scale, 1, image.width / fullWidth);
        const moving = now - movedAt < SETTLE_MS;

        // Step 1: decide what the sharp region needs, and put it off while the
        // view is moving and a fresh one would cost more than a frame.
        let next: ChainRegion | null = null;
        if (visible) {
            const action = detailAction(detail?.region ?? null, detailStale, visible.image, scale, moving, lastDetailMs < CHEAP_MS);
            if (action === "render") {
                const needed = neededBox(visible.image, scale);
                const whole = neededBox([0, 0, fullWidth, fullHeight], scale);
                const margin = contains(needed, whole) ? 0 : PAN_MARGIN;
                const apron = sharpnessReach(currentSliders, scale, fullWidth, fullHeight);
                const limit = gpu.device.limits.maxTextureDimension2D;
                next = planRegion(needed, scale, apron, margin, fullWidth, fullHeight, limit);
                const same = detail && !detailStale && detail.region.scale === next.scale && next.box.every((v, i) => v === detail!.region.box[i]);
                if (same) next = null;
            } else if (action === "defer") {
                counters.deferred++;
                settleTimer = later(settleTimer, SETTLE_MS - (now - movedAt));
            }
        }

        // Step 2: whether the sharp region, as it will be after this frame,
        // covers everything on screen. Wherever it does not, the backdrop does.
        const sharp = next ?? detail?.region ?? null;
        const drawSharp = !!sharp && !!visible && showsDetail(sharp, scale, backdrop?.region ?? null);
        const covered = drawSharp && contains(validBox(sharp!), neededBox(visible!.image, sharp!.scale));
        const needBackdrop = !!visible && !covered;
        let renderedBackdrop = false;
        if (needBackdrop && (backdropStale || !backdrop)) {
            renderBackdrop(view, image);
            renderedBackdrop = true;
        }

        // Step 3: the canvas texture, and the sharp region's chain, recorded
        // after any backdrop run was submitted so each has its own uniforms.
        const targetTexture = canvas.canvasConfig.getCurrentTexture();
        const acquireEnd = measuring ? performance.now() : 0;
        const encoder = gpu.device.createCommandEncoder({ label: "Render Frame" });

        if (next) {
            detail = outputFor(detail, next, "Sharp Region");
            recordChain(encoder, view, image, next, detail.texture, true);
            detailStale = false;
            counters.detailRenders++;
        }

        // Step 4: the output transform onto the canvas — the backdrop first
        // where it is needed, then the sharp region over it.
        const drawn = visible ? pixelBox(visible.canvas, targetTexture.width, targetTexture.height) : null;
        const targetView = targetTexture.createView();
        let cleared = false;
        if (drawn && needBackdrop && backdrop) {
            writeCrop(backdrop, view, imageInChain(backdrop.region, fullWidth, fullHeight));
            recordFullScreenPass(encoder, colorSpaceEncode, targetView, backdrop.encode, {
                ...passOptions("backdrop", drawn),
                clearValue: CLEAR,
            });
            cleared = true;
        }
        if (drawn && drawSharp && detail) {
            const imageBox = imageInChain(detail.region, fullWidth, fullHeight);
            const valid = validBox(detail.region);
            const kept = covered
                ? imageBox
                : intersect(imageBox, [
                      valid[0] - detail.region.box[0] + 1,
                      valid[1] - detail.region.box[1] + 1,
                      valid[2] - detail.region.box[0] - 1,
                      valid[3] - detail.region.box[1] - 1,
                  ]);
            if (kept) {
                writeCrop(detail, view, kept);
                recordFullScreenPass(encoder, cleared ? colorSpaceEncodeOver : colorSpaceEncode, targetView, detail.encode, {
                    ...passOptions("encode", drawn),
                    clearValue: CLEAR,
                    load: cleared,
                });
                cleared = true;
            }
        }
        if (!cleared) {
            const pass = encoder.beginRenderPass({
                label: "Clear Pass",
                colorAttachments: [{ view: targetView, clearValue: CLEAR, loadOp: "clear", storeOp: "store" }],
            });
            pass.end();
        }

        if (measuring) frameTimer.resolve(encoder);

        // Step 5: submit, and hold the next frame until this one has drained.
        const submitAt = performance.now();
        gpu.device.queue.submit([encoder.finish()]);

        inFlight = true;
        const drained = gpu.device.queue.onSubmittedWorkDone();
        const timedDetail = !!next && !renderedBackdrop;
        drained.then(() => {
            if (timedDetail) lastDetailMs = performance.now() - submitAt;
        });

        // Once idle, the backdrop catches up with the adjustments, so a later
        // gesture finds it ready.
        if (backdropStale && !renderedBackdrop) {
            idleTimer = later(idleTimer, BACKDROP_IDLE_MS);
            if (!moving && !next && now - movedAt > BACKDROP_IDLE_MS) renderBackdrop(view, image);
        }

        if (previewDebug.enabled) report(view, visible?.image ?? null, needBackdrop);

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

    function report(view: ViewFrame, visible: Box | null, backdropShown: boolean) {
        previewDebug.state = {
            fullWidth: view.fullWidth,
            fullHeight: view.fullHeight,
            visible,
            scale: view.scale,
            detail: detail?.region ?? null,
            backdrop: backdrop?.region ?? null,
            backdropShown,
            source: {
                width: source!.width,
                height: source!.height,
                mipLevels: source!.texture.mipLevelCount,
            },
        };
        previewDebug.counters = { ...counters, lastDetailMs };
    }

    // Releases what the renderer allocated. The source texture is not included:
    // it is created by the caller and handed in, and the caller destroys it —
    // both when swapping images and on teardown.
    function destroy() {
        destroyed = true;
        dirty = false;
        if (frameHandle !== null) cancelAnimationFrame(frameHandle);
        frameHandle = null;
        for (const timer of [settleTimer, idleTimer]) if (timer !== null) clearTimeout(timer);
        settleTimer = idleTimer = null;

        calcParams.destroy();
        outputSpace.destroy();
        viewSampleBuffer.destroy();
        frameTimer.destroy();
        destroyScratch();
        releaseHeld(detail);
        releaseHeld(backdrop);
        detail = backdrop = null;
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
