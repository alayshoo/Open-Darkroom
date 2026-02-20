export interface Adjustments {
  wb_temp: number;
  wb_tint: number;
  exposure: number;
  contrast: number;
  brightness: number;
  saturation: number;
  vibrance: number;
}

export const defaultAdjustments: Adjustments = {
  wb_temp: 0,
  wb_tint: 0,
  exposure: 0,
  contrast: 0,
  brightness: 0,
  saturation: 0,
  vibrance: 0
};