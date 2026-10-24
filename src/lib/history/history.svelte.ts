import type { Action } from "$lib/types/historyActions";

class History {
  stack: Action[] = $state([]);
  pointer: number = $state(-1); // points at last applied action

  get canUndo() {
    return this.pointer >= 0;
  }

  get canRedo() {
    return this.pointer < this.stack.length - 1;
  }

  push(action: Action) {
    // discard any "future" actions after current pointer
    this.stack = this.stack.slice(0, this.pointer + 1);
    this.stack.push(action);
    this.pointer++;
  }

  undo(): Action | null {
    if (!this.canUndo) return null;
    return this.stack[this.pointer--];
  }

  redo(): Action | null {
    if (!this.canRedo) return null;
    return this.stack[++this.pointer];
  }

  /** Drop every action matching `pred`, keeping the pointer on the same step. */
  discard(pred: (a: Action) => boolean) {
    const removedBefore = this.stack.slice(0, this.pointer + 1).filter(pred).length;
    this.stack = this.stack.filter((a) => !pred(a));
    this.pointer -= removedBefore;
  }

  clear() {
    this.stack = [];
    this.pointer = -1;
  }
}

export const history = new History();