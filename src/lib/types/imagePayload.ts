export interface ImagePayload {
    // Preview dimensions — the size of the pixel buffer below.
    width: number;
    height: number;
    // Dimensions of the image the preview was downscaled from. Sliders measured
    // in image pixels are anchored to these.
    fullWidth: number;
    fullHeight: number;
    pixels: Uint8Array;
    // Overview to feed the histogram
    overviewWidth: number;
    overviewHeight: number;
    overviewPixels: Uint8Array;
    histR: Uint32Array;
    histG: Uint32Array;
    histB: Uint32Array;
    // Source RGB → linear Rec.2020, column-major. The darkroom block runs in
    // the file's own primaries; this converts out of them afterwards.
    srcToWorking: Float32Array;
    // Set when the backend had to apply the profile itself, so the pixels are
    // already in the working space and srcToWorking is the identity.
    colourBaked: boolean;
    // Tags this open, so bands streamed for it are refused once it is replaced.
    imageId: number;
}
