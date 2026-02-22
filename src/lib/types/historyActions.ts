// lib/types/historyActions.ts

import type { Adjustments } from "$lib/types/adjustments";

export type Action =
  | {
      type: "slider";
      key: keyof Adjustments;
      oldValue: number;
      newValue: number;
    }
  // future-proof:
  // | { type: 'mask-draw'; maskId: string; strokeData: ... }
  // | { type: 'crop'; oldRect: Rect; newRect: Rect }
  ;