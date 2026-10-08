/**
 * The host's mutation observer (PX-122). A script injected into every
 * document the session loads — in an isolated world, so the page's own
 * script can neither see it nor disable it (it has its own prototypes and
 * globals; the page can delete `MutationObserver` from its world and this
 * one is untouched) — watches the DOM and reports through a binding that
 * exists only in that world. The script folds a burst of mutations into
 * one report at most every 100 ms; the host folds the reports again into a
 * notice at most every `NOTICE_EVERY_MS`, whatever the page does. The
 * Core turns a notice into a semantic delta; the notice carries counts, never
 * page content.
 */

/** The isolated world the observer lives in. */
export const OBSERVER_WORLD = "modbit-observer";
/** The binding it reports through. */
export const OBSERVER_BINDING = "__modbitObserverReport";
/** Notices leave the host at most this often. */
export const NOTICE_EVERY_MS = 250;

/** The source injected into every document (and run in the current one). */
export const OBSERVER_SOURCE = `(() => {
  if (globalThis.__modbitObserverInstalled) return;
  globalThis.__modbitObserverInstalled = true;
  const report = (o) => { try { ${OBSERVER_BINDING}(JSON.stringify(o)); } catch (e) {} };
  let added = 0, removed = 0, attributes = 0, text = 0, folded = 0, focus = false, timer = null;
  const flush = () => {
    timer = null;
    if (added + removed + attributes + text === 0 && !focus) return;
    const r = { kind: 'mutation', added, removed, attributes, text, focus, coalesced: folded };
    added = removed = attributes = text = folded = 0; focus = false;
    report(r);
  };
  const arm = () => { if (timer === null) timer = setTimeout(flush, 100); };
  const observe = () => {
    const mo = new MutationObserver((records) => {
      for (const m of records) {
        folded += 1;
        if (m.type === 'childList') { added += m.addedNodes.length; removed += m.removedNodes.length; }
        else if (m.type === 'attributes') attributes += 1;
        else text += 1;
      }
      arm();
    });
    mo.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
    document.addEventListener('focusin', () => { focus = true; arm(); }, true);
    document.addEventListener('focusout', () => { focus = true; arm(); }, true);
  };
  if (document) observe();
  addEventListener('pageshow', () => report({ kind: 'load', coalesced: 0 }));
})();`;

export interface Notice {
  change_seq: number;
  kind: string;
  added: number;
  removed: number;
  attributes: number;
  text: number;
  focus: boolean;
  frame: string | null;
  coalesced: number;
}

/**
 * Folds the observer's reports into bounded notices: whatever arrives within
 * a window leaves as one, the window never shorter than `everyMs`, the
 * counter monotonic. `emit` is called at most once per window.
 */
export class NoticeBatcher {
  private seq = 0;
  private pending: Omit<Notice, "change_seq"> | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private last = 0;
  private readonly emit: (n: Notice) => void;
  private readonly everyMs: number;
  /** Reports folded in total (the witness of the rate bound). */
  reports = 0;
  /** Notices emitted. */
  emitted = 0;

  constructor(emit: (n: Notice) => void, everyMs = NOTICE_EVERY_MS) {
    this.emit = emit;
    this.everyMs = everyMs;
  }

  /** The counter now: what a snapshot taken now covers. */
  get changeSeq(): number {
    return this.seq;
  }

  /** A report from the observer (or a navigation the host saw). */
  report(r: { kind?: string; added?: number; removed?: number; attributes?: number; text?: number; focus?: boolean; coalesced?: number; frame?: string | null }): void {
    this.reports += 1;
    this.seq += 1;
    const p = this.pending ?? { kind: "mutation", added: 0, removed: 0, attributes: 0, text: 0, focus: false, frame: null, coalesced: 0 };
    p.added += r.added ?? 0;
    p.removed += r.removed ?? 0;
    p.attributes += r.attributes ?? 0;
    p.text += r.text ?? 0;
    p.focus = p.focus || r.focus === true;
    p.coalesced += r.coalesced ?? 1;
    // A navigation or a load outranks a mutation in the notice's kind.
    if (r.kind === "navigation" || (r.kind === "load" && p.kind !== "navigation")) p.kind = r.kind;
    p.frame = r.frame ?? p.frame;
    this.pending = p;
    if (this.timer === null) {
      const wait = Math.max(0, this.last + this.everyMs - Date.now());
      this.timer = setTimeout(() => this.flush(), wait);
    }
  }

  /** Emit what is pending now (the timer's job; also the end of a session). */
  flush(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    const p = this.pending;
    this.pending = null;
    if (!p) return;
    this.last = Date.now();
    this.emitted += 1;
    this.emit({ ...p, change_seq: this.seq });
  }

  stop(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.pending = null;
  }
}
