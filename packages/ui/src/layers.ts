/**
 * The layer stack (AFW-H01, PX-045): every popover, menu, tooltip, dialog and
 * palette that is open is a layer, and Escape closes the innermost one and
 * nothing else. Pure; one window key listener in the app calls `escape()`.
 */
export interface Layer {
  id: string;
  /** A modal layer blocks the shell's shortcuts while it is open. */
  modal: boolean;
  onClose: () => void;
}

export class LayerStack {
  private stack: Layer[] = [];

  /** Pushes a layer; the returned function removes exactly that layer (idempotent). */
  push(layer: Layer): () => void {
    this.stack.push(layer);
    return () => {
      const i = this.stack.indexOf(layer);
      if (i >= 0) this.stack.splice(i, 1);
    };
  }

  get size(): number {
    return this.stack.length;
  }

  top(): Layer | undefined {
    return this.stack[this.stack.length - 1];
  }

  hasModal(): boolean {
    return this.stack.some((l) => l.modal);
  }

  ids(): string[] {
    return this.stack.map((l) => l.id);
  }

  /** Closes the innermost layer; true when there was one (the key is then consumed). */
  escape(): boolean {
    const top = this.top();
    if (!top) return false;
    top.onClose();
    return true;
  }
}

export const layers = new LayerStack();
