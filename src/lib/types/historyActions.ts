// lib/types/historyActions.ts

import type { Sliders } from "$lib/types/imgParameters";
import type { Curves } from "$lib/types/curves";
import type { Crop } from "$lib/types/crop";

export type Action =
  | {
      type: "slider";
      key: keyof Sliders;
      oldValue: number | boolean;
      newValue: number | boolean;
    }
  // Many sliders moving as one step, so a paste or a reset takes a single undo.
  // The curves ride along when the set includes them.
  | {
      type: "sliders";
      oldValues: Partial<Sliders>;
      newValues: Partial<Sliders>;
      oldCurves?: Curves;
      newCurves?: Curves;
    }
  // One edit in the curves editor, from pointer down to pointer up
  | {
      type: "curves";
      oldCurves: Curves;
      newCurves: Curves;
    }
  | {
      type: "crop";
      oldValue: Crop;
      newValue: Crop;
    }
  // future-proof:
  // | { type: 'mask-draw'; maskId: string; strokeData: ... }
  ;