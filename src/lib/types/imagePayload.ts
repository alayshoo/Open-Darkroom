export interface ImagePayload {
    width: number;
    height: number;
    pixels: Uint8Array;
    histR: Uint32Array;
    histG: Uint32Array;
    histB: Uint32Array;
}
