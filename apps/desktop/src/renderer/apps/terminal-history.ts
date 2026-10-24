/**
 * Older terminal output (REQ-PX-048, PX-043), DOM-free so the node tests drive
 * it against a fake reader: the output that scrolled out of the replay window
 * is read from the Core's sealed output as cursor ranges, newest range first,
 * and kept in cursor order. Each range is taken once: a second request while
 * one is in flight shares it, and a range that does not end exactly where the
 * loaded text begins is refused rather than guessed into place.
 *
 * The renderer holds no authority: the Core decides what was retained and
 * where it starts; this only asks for the range just below what it has.
 */
export interface HistoryRange {
  /** The first cursor the Core retained: nothing is older. */
  startCursor: bigint;
  /** The cursor the returned bytes begin at (the request, clamped to `startCursor`). */
  from: bigint;
  data: Uint8Array;
}
export type HistoryReader = (from: bigint, to: bigint) => Promise<HistoryRange>;

export interface HistoryState {
  /** Everything in [loadedFrom, origin) is loaded, in order. */
  loadedFrom: bigint;
  origin: bigint;
  pages: number;
  bytes: number;
  exhausted: boolean;
  loading: boolean;
  error: string | null;
}

export const HISTORY_PAGE_BYTES = 64 * 1024;

export type HistoryOutcome = "loaded" | "exhausted" | "failed";

export class HistoryLoader {
  private pages: Uint8Array[] = [];
  private low: bigint;
  private done: boolean;
  private inflight: Promise<HistoryOutcome> | null = null;
  private error: string | null = null;
  private readonly read: HistoryReader;
  private readonly origin: bigint;
  private readonly pageBytes: bigint;
  private readonly onChange: () => void;

  constructor(read: HistoryReader, origin: bigint, onChange: () => void, pageBytes = HISTORY_PAGE_BYTES) {
    this.read = read;
    this.origin = origin;
    this.low = origin;
    this.done = origin === 0n;
    this.pageBytes = BigInt(pageBytes);
    this.onChange = onChange;
  }

  snapshot(): HistoryState {
    return { loadedFrom: this.low, origin: this.origin, pages: this.pages.length, bytes: this.pages.reduce((n, p) => n + p.length, 0), exhausted: this.done, loading: this.inflight !== null, error: this.error };
  }

  /** The loaded bytes, oldest first. */
  bytes(): Uint8Array {
    const out = new Uint8Array(this.pages.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const p of this.pages) {
      out.set(p, at);
      at += p.length;
    }
    return out;
  }

  /** Reads the range just below what is loaded. Concurrent calls share one read. */
  loadOlder(): Promise<HistoryOutcome> {
    if (this.done) return Promise.resolve("exhausted");
    if (this.inflight) return this.inflight;
    const to = this.low;
    const from = to > this.pageBytes ? to - this.pageBytes : 0n;
    this.error = null;
    const run = (async (): Promise<HistoryOutcome> => {
      try {
        const r = await this.read(from, to);
        if (r.data.length === 0) {
          this.done = true;
          return "exhausted";
        }
        // The range must end exactly where the loaded text begins, or it would be a gap or a repeat.
        if (r.from + BigInt(r.data.length) !== to) throw new Error(`HISTORY_RANGE_MISMATCH: asked for the bytes before ${to}, received ${r.from}..${r.from + BigInt(r.data.length)}`);
        this.pages.unshift(r.data);
        this.low = r.from;
        if (r.from <= r.startCursor || r.from === 0n) this.done = true;
        return this.done ? "exhausted" : "loaded";
      } catch (e) {
        this.error = (e as Error).message;
        return "failed";
      }
    })().finally(() => {
      this.inflight = null;
      this.onChange();
    });
    this.inflight = run;
    this.onChange();
    return run;
  }
}

/** The terminal's own control sequences removed and line ends normalised: what a person reads in the earlier-output pane. */
export function historyText(bytes: Uint8Array): string {
  const text = new TextDecoder("utf-8").decode(bytes);
  return text
    .replace(/\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)/g, "")
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\u001b[@-Z\\-_]/g, "")
    .replace(/\r\n?/g, "\n");
}
