// lib/types/historyActions.ts

import type { Sliders } from "$lib/types/imgParameters";

export type Action =
  | {
      type: "slider";
      key: keyof Sliders;
      oldValue: number | boolean;
      newValue: number | boolean;
    }
  // future-proof:
  // | { type: 'mask-draw'; maskId: string; strokeData: ... }
  // | { type: 'crop'; oldRect: Rect; newRect: Rect }
  ;