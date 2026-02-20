// src/lib/gpu/renderer.ts

import type { Adjustments } from "../types/adjustments";
import type { GPUContext } from "./context";
import type { GPUImage } from "./texture";
import {
    createImgDevPipeline,
    updateParams,
    type ImgDevPipeline,
} from "./pipelines/imgDevPipeline";

export interface Renderer {
    loadImage: (image: GPUImage) => void;
    setAdjustments: (adjustments: Adjustments) => void;
    render: () => void;
    destroy: () => void;
}

export function createRenderer(gpu: GPUContext): Renderer {
    const { device, context, format } = gpu;

    // Create our pipeline(s) once at startup
    const imgDev: ImgDevPipeline = createImgDevPipeline(
        device,
        format
    );

    // The bind group connects actual resources to the layout slots.
    // We can't create it until we have an image, so it starts null.
    let bindGroup: GPUBindGroup | null = null;
    let currentImage: GPUImage | null = null;

    function loadImage(image: GPUImage) {
        currentImage = image;

        // NOW we can create the bind group — it connects:
        //   binding 0 → the actual texture view
        //   binding 1 → the actual sampler
        //   binding 2 → the actual params buffer
        bindGroup = device.createBindGroup({
            label: "Image Development Bind Group",
            layout: imgDev.bindGroupLayout,
            entries: [
                {
                    binding: 0,
                    resource: image.texture.createView(),
                },
                {
                    binding: 1,
                    resource: imgDev.sampler,
                },
                {
                    binding: 2,
                    resource: { buffer: imgDev.paramsBuffer },
                },
            ],
        });
    }

    function setAdjustments(adjustments: Adjustments) {
        updateParams(device, imgDev.paramsBuffer, adjustments);
    }

    function render() {
        if (!bindGroup || !currentImage) return;

        // Step 1: Get the current canvas texture to render into.
        // This changes every frame (it's a swapchain texture).
        const targetTexture = context.getCurrentTexture();
        const targetView = targetTexture.createView();

        // Step 2: Create a command encoder — this records GPU commands.
        // Nothing actually executes until we submit the command buffer.
        const encoder = device.createCommandEncoder({
            label: "Render Frame",
        });

        // Step 3: Begin a render pass.
        // A render pass = "draw things into this texture."
        const pass = encoder.beginRenderPass({
            colorAttachments: [
                {
                    view: targetView,
                    clearValue: { r: 0.1, g: 0.1, b: 0.1, a: 1 },
                    loadOp: "clear",                                    // Clear to dark gray before drawing
                    storeOp: "store",                                   // Store the results
                },
            ],
        });

        // Step 4: Record draw commands
        pass.setPipeline(imgDev.pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.draw(6); // 6 vertices = full-screen quad (2 triangles)

        // Step 5: End the pass and submit
        pass.end();
        device.queue.submit([encoder.finish()]);
    }

    function destroy() {
        imgDev.paramsBuffer.destroy();
        currentImage?.texture.destroy();
    }

    // Initialize exposure to 0 (no change)
    setAdjustments({ wbTemp: 0, wbTint: 0, exposure: 0, contrast: 0, brightness: 0, vibrance: 0, saturation:0 });

    return { loadImage, setAdjustments, render, destroy };
}