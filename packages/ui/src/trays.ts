/**
 * The tray store (AFW-H01): transient attention UI that docks above the
 * composer and is never a modal. Several trays may be pending; exactly one is
 * active and owns the scoped keybindings of its actions. Trays persist across
 * navigation (this store lives above the views) until resolved or dismissed.
 */
export type TrayTone = "info" | "warn" | "error" | "attention";

export interface TrayAction {
  id: string;
  label: string;
  /** A chord active only while this tray is the active one (e.g. `mod+enter`). */
  keys?: string | undefined;
  primary?: boolean | undefined;
  run: () => void;
}

export interface TrayDef<B = unknown> {
  id: string;
  tone: TrayTone;
  title: string;
  body?: B | undefined;
  actions?: readonly TrayAction[] | undefined;
  /** Whether the person may dismiss it (an approval is resolved, not dismissed). Default true. */
  dismissible?: boolean | undefined;
  /** Higher wins the active slot. Default 0. */
  priority?: number | undefined;
}

export interface TraySnapshot<B = unknown> {
  trays: readonly TrayDef<B>[];
  activeId: string | null;
}

export class TrayStore<B = unknown> {
  private trays: TrayDef<B>[] = [];
  private seq = new Map<string, number>();
  private counter = 0;
  private activeId: string | null = null;
  private listeners = new Set<() => void>();
  private snap: TraySnapshot<B> = { trays: [], activeId: null };

  private publish(): void {
    this.snap = { trays: [...this.trays], activeId: this.activeId };
    for (const l of this.listeners) l();
  }

  private best(): string | null {
    let best: TrayDef<B> | null = null;
    for (const t of this.trays) {
      if (!best) best = t;
      else {
        const dp = (t.priority ?? 0) - (best.priority ?? 0);
        if (dp > 0 || (dp === 0 && (this.seq.get(t.id) ?? 0) > (this.seq.get(best.id) ?? 0))) best = t;
      }
    }
    return best?.id ?? null;
  }

  subscribe = (fn: () => void): (() => void) => {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  };

  getSnapshot = (): TraySnapshot<B> => this.snap;

  /** Presents (or replaces, by id) a tray. A new tray becomes active unless the active one outranks it; refreshing a tray already shown never moves the slot, so a tray the person brought forward stays forward while the others update. */
  present(def: TrayDef<B>): void {
    const existing = this.trays.findIndex((t) => t.id === def.id);
    const isNew = existing < 0;
    if (isNew) {
      this.trays.push(def);
      this.seq.set(def.id, ++this.counter);
    } else this.trays[existing] = def;
    const active = this.trays.find((t) => t.id === this.activeId);
    if (!active || (isNew && (def.priority ?? 0) >= (active.priority ?? 0))) this.activeId = def.id;
    this.publish();
  }

  /** Resolves or dismisses a tray; the next best one becomes active. */
  dismiss(id: string): void {
    const i = this.trays.findIndex((t) => t.id === id);
    if (i < 0) return;
    this.trays.splice(i, 1);
    this.seq.delete(id);
    if (this.activeId === id) this.activeId = this.best();
    this.publish();
  }

  /** The person brings a pending tray forward. */
  activate(id: string): void {
    if (!this.trays.some((t) => t.id === id) || this.activeId === id) return;
    this.activeId = id;
    this.publish();
  }

  active(): TrayDef<B> | null {
    return this.trays.find((t) => t.id === this.activeId) ?? null;
  }
}

export const trays = new TrayStore();
