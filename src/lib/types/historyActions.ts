// lib/types/historyActions.ts

import type { Sliders } from "$lib/types/imgParameters";

export type Action =
  | {
      type: "slider";
      key: keyof Sliders;
      oldValue: number | boolean;
      newValue: number | boolean;
    }
  // Many sliders moving as one step, so a paste or a reset takes a single undo
  | {
      type: "sliders";
      oldValues: Partial<Sliders>;
      newValues: Partial<Sliders>;
    }
  // future-proof:
  // | { type: 'mask-draw'; maskId: string; strokeData: ... }
  // | { type: 'crop'; oldRect: Rect; newRect: Rect }
  ;