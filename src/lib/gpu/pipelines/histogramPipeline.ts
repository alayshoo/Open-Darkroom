// src/lib/gpu/pipelines/histogramPipeline.ts

import type { Adjustments } from "$lib/types/adjustments";

export interface HistogramPipeline {
    computeHistogram: (inputTexture: GPUTexture) => Promise<{
        r: Uint32Array;
        g: Uint32Array; 
        b: Uint32Array;
    }>;
    destroy: () => void;
}

