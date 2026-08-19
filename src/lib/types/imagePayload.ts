export interface ImagePayload {
    // Preview dimensions — the size of the pixel buffer below.
    width: number;
    height: number;
    // Dimensions of the image the preview was downscaled from. Sliders measured
    // in image pixels are anchored to these.
    fullWidth: number;
    fullHeight: number;
    pixels: Uint8Array;
    // Whole-frame overview, same RGBA f16 layout as the preview. The live
    // histogram reads this so it keeps describing the whole image whatever the
    // preview is showing.
    overviewWidth: number;
    overviewHeight: number;
    overviewPixels: Uint8Array;
    histR: Uint32Array;
    histG: Uint32Array;
    histB: Uint32Array;
}
