export interface Adjustments {
  wbTemp: number;
  wbTint: number;
  exposure: number;
  contrast: number;
  brightness: number;
  saturation: number;
  vibrance: number;
}

export const defaultAdjustments: Adjustments = {
  wbTemp: 0,
  wbTint: 0,
  exposure: 0,
  contrast: 0,
  brightness: 0,
  saturation: 0,
  vibrance: 0
};

// Maps UI slider values to shader-expected ranges
export function mapSlidersToAdjustments(sliders: {
  wbTemp: number;  // 1200–10000 K
  wbTint: number;         // -100 to 100
  exposure: number;     // -5 to 5 (already correct)
  contrast: number;     // -100 to 100
  brightness: number;   // -10 to 10
  vibrance: number;     // -100 to 100
  saturation: number;   // -100 to 100
}): Adjustments {
  return {
    wbTemp: (sliders.wbTemp - 5600) / 4400, // maps 1200→-1, 5600→0, 10000→+1
    wbTint: sliders.wbTint / 100,
    exposure: sliders.exposure,
    contrast: sliders.contrast / 200 + 0.5,
    brightness: sliders.brightness / 10,
    saturation: sliders.saturation / 100,
    vibrance: sliders.vibrance / 100,
  };
}