// $lib/config/slidersConfig.ts

export const SLIDER_DEFAULTS = {
    wbTemp: 5600,
    wbTint: 0,
    exposure: 0,
    contrast: 0,
    brightness: 0,
    saturation: 0,
    vibrance: 0,
  } as const;
  
  export type Sliders = { -readonly [K in keyof typeof SLIDER_DEFAULTS]: number };
  
  export const COLOR_KEYS = [
    "saturation",
    "vibrance",
    "wbTemp",
    "wbTint",
  ] as const;
  
  export const BW_TARGETS: Partial<Sliders> = {
    saturation: -100,
    vibrance: 0,
    wbTemp: 5600,
    wbTint: 0,
  };