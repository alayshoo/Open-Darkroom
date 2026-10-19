// $lib/types/toneMap.ts

/** How the highlight roll-off compresses values above its shoulder. */
export type RolloffMode = "luminance" | "perChannel";

/** App-wide tone-mapping preference, applied at the end of the develop chain. */
export interface ToneMap {
    /** 0–100; 0 leaves the chain untouched. */
    rolloff: number;
    rolloffMode: RolloffMode;
}

export const defaultToneMap: ToneMap = {
    rolloff: 0,
    rolloffMode: "luminance",
};
