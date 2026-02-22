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

  clear() {
    this.stack = [];
    this.pointer = -1;
  }
}

export const history = new History();