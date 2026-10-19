// $lib/types/toneMap.ts

/** How the highlight roll-off compresses values above its shoulder. */
export type RolloffMode = "luminance" | "perChannel";

/** Colour model the per-colour HSL bands are measured and adjusted in. */
export type HslModel = "oklch" | "hsv";

/** App-wide rendering preferences tuned from the debug panel. */
export interface ToneMap {
    /** 0–100; 0 leaves the chain untouched. */
    rolloff: number;
    rolloffMode: RolloffMode;
    hslModel: HslModel;
}

export const defaultToneMap: ToneMap = {
    rolloff: 0,
    rolloffMode: "luminance",
    hslModel: "oklch",
};
