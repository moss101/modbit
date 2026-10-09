/**
 * The Terminal app's session logic (REQ-PX-048, AFW-I03), DOM-free so the node
 * tests drive it against a fake bridge: attach by cursor, write what arrives to
 * a sink, acknowledge only what the sink has finished consuming, reattach from
 * the consumed cursor when the stream ends without the process ending, take the
 * person's input lease on the first keystroke and give it back on blur.
 *
 * The renderer holds no authority and no terminal state: the cursor here is a
 * position in the Core's output, and every byte on screen came from a frame.
 */
import type { ModbitBridge, TerminalFrameJson, TerminalViewJson } from "../../preload/preload.ts";

export type TerminalBridge = Pick<ModbitBridge, "terminalAttach" | "terminalAck" | "terminalDetach" | "terminalInput" | "terminalResize" | "terminalWrite" | "onTerminalFrame">;

/** Where output goes. `done` runs when the sink has consumed the bytes (parsed them, for xterm). */
export interface TerminalSink {
  write(data: Uint8Array, done: () => void): void;
  /** Clear the screen before a replay from an earlier cursor. */
  reset(): void;
  /** A line the view shows apart from the terminal's own output. */
  notice(text: string): void;
}

export type TerminalPhase = "idle" | "attaching" | "live" | "exited" | "lost" | "refused";

export interface TerminalViewState {
  phase: TerminalPhase;
  /** The next byte the view expects: everything before it is on the screen. */
  cursor: bigint;
  /** This view holds the terminal's input lease: the agent's own input is refused meanwhile. */
  leaseHeld: boolean;
  exit: { exitCode: number | null; signal: number | null; timedOut: boolean; cancelled: boolean } | null;
  /** Why the view is refused or lost, in the Core's words. */
  reason: string | null;
  /** Bytes on screen since the first attach (for tests and the status line). */
  bytes: bigint;
}

export interface TerminalTarget {
  sessionId: string;
  taskId: string;
  terminalId: string;
}

/** Reasons a stream ends without the process ending, after which the view attaches again from its cursor. */
const REATTACH = new Set(["SLOW_CONSUMER", "STALE_GENERATION", "BROKER_LOST", "BROKER_ERROR", "CORE_RESTARTED"]);
const MAX_QUEUED_FRAMES = 4096;

export class TerminalController {
  private state: TerminalViewState = { phase: "idle", cursor: 0n, leaseHeld: false, exit: null, reason: null, bytes: 0n };
  private attachId: string | null = null;
  private queued: TerminalFrameJson[] = [];
  private off: (() => void) | null = null;
  private ackTarget = 0n;
  private ackSent = 0n;
  private ackBusy = false;
  private disposed = false;
  private generation = 0;
  private chain: Promise<unknown> = Promise.resolve();
  private size: { rows: number; cols: number } | null = null;
  private wantLease = false;
  /** The Core listed the process as running when this view attached; an ended process takes no resize or keystroke. */
  private running = true;
  private resizeRetries = 0;
  private reattachTimer: ReturnType<typeof setTimeout> | null = null;

  private readonly bridge: TerminalBridge;
  private readonly sink: TerminalSink;
  private readonly target: TerminalTarget;
  private readonly onChange: (s: TerminalViewState) => void;
  private readonly opts: { windowBytes?: number; reattachDelayMs?: number; resizeRetryMs?: number };

  constructor(bridge: TerminalBridge, sink: TerminalSink, target: TerminalTarget, onChange: (s: TerminalViewState) => void, opts: { windowBytes?: number; reattachDelayMs?: number; resizeRetryMs?: number } = {}) {
    this.bridge = bridge;
    this.sink = sink;
    this.target = target;
    this.onChange = onChange;
    this.opts = opts;
  }

  snapshot(): TerminalViewState {
    return this.state;
  }

  /** Output moves the cursor on every frame; the view hears about it once per burst, not once per chunk. */
  private notifyQueued = false;
  private notifySoon(): void {
    if (this.notifyQueued) return;
    this.notifyQueued = true;
    queueMicrotask(() => {
      this.notifyQueued = false;
      if (!this.disposed) this.onChange(this.state);
    });
  }

  private set(patch: Partial<TerminalViewState>): void {
    this.state = { ...this.state, ...patch };
    this.onChange(this.state);
  }

  /** Attach from `fromCursor` (the oldest replayable byte replays the whole window) and follow the stream. */
  async start(fromCursor: bigint): Promise<void> {
    this.off ??= this.bridge.onTerminalFrame((f) => this.onFrame(f));
    this.sink.reset();
    this.state = { ...this.state, bytes: 0n };
    await this.attach(fromCursor);
  }

  private async attach(fromCursor: bigint): Promise<void> {
    if (this.disposed) return;
    const mine = ++this.generation;
    this.attachId = null;
    this.queued = [];
    this.ackSent = fromCursor;
    this.ackTarget = fromCursor;
    this.set({ phase: "attaching", cursor: fromCursor, reason: null });
    try {
      const a = await this.bridge.terminalAttach(this.target.sessionId, this.target.taskId, this.target.terminalId, fromCursor.toString(), {
        ...(this.opts.windowBytes ? { windowBytes: this.opts.windowBytes } : {}),
        takeInputLease: this.wantLease,
      });
      if (this.disposed || mine !== this.generation) {
        void this.bridge.terminalDetach(a.attachId).catch(() => undefined);
        return;
      }
      this.attachId = a.attachId;
      this.running = a.terminal.state === "RUNNING";
      // A terminal that already ended replays its output and then its exit frame; until that frame the view is live.
      this.set({ phase: "live", leaseHeld: a.inputLeaseHeld });
      if (this.size) void this.resize(this.size.rows, this.size.cols, true);
      // Frames that arrived before the attach answered are in the queue, in order.
      const early = this.queued;
      this.queued = [];
      for (const f of early) if (f.attachId === a.attachId) this.apply(f);
    } catch (e) {
      const msg = (e as Error).message;
      const expired = /CURSOR_EXPIRED/.test(msg) ? /oldest_cursor=(\d+)/.exec(msg) : null;
      if (expired) {
        // The replay window moved past the cursor: show what is still replayable, and say what was dropped.
        this.sink.reset();
        this.sink.notice(`older output is no longer replayable; showing from byte ${expired[1]}`);
        await this.attach(BigInt(expired[1]!));
        return;
      }
      this.set({ phase: "refused", reason: msg });
    }
  }

  private onFrame(f: TerminalFrameJson): void {
    if (this.disposed) return;
    if (this.attachId === null) {
      if (this.state.phase === "attaching" && this.queued.length < MAX_QUEUED_FRAMES) this.queued.push(f);
      return;
    }
    if (f.attachId !== this.attachId) return;
    this.apply(f);
  }

  private apply(f: TerminalFrameJson): void {
    switch (f.kind) {
      case "output": {
        const at = BigInt(f.cursor);
        const have = this.state.cursor;
        // The stream is ordered; bytes already on screen (a replay overlap) are skipped, a gap is never filled with a guess.
        if (at + BigInt(f.data.length) <= have) return;
        if (at > have) {
          this.sink.notice(`output gap: expected byte ${have}, received ${at}`);
        }
        const skip = at < have ? Number(have - at) : 0;
        const data = skip > 0 ? f.data.subarray(skip) : f.data;
        const next = (at < have ? have : at) + BigInt(data.length);
        this.state = { ...this.state, cursor: next, bytes: this.state.bytes + BigInt(data.length) };
        this.notifySoon();
        this.sink.write(data, () => this.consumed(next));
        return;
      }
      case "exit":
        this.running = false;
        this.set({ phase: "exited", exit: { exitCode: f.exitCode, signal: f.signal, timedOut: f.timedOut, cancelled: f.cancelled }, leaseHeld: false });
        return;
      case "lease":
        this.set({ leaseHeld: f.held && f.holder.startsWith("user:") });
        return;
      case "ended": {
        this.attachId = null;
        if (f.reason === "CURSOR_EXPIRED") {
          this.sink.reset();
          this.sink.notice(`older output is no longer replayable; showing from byte ${f.oldestCursor}`);
          void this.attach(BigInt(f.oldestCursor));
          return;
        }
        if (REATTACH.has(f.reason) && this.state.phase !== "exited") {
          // Resume from what is on screen, not from the Core's idea of it.
          this.set({ phase: "attaching", leaseHeld: false });
          this.reattachTimer = setTimeout(() => void this.attach(this.state.cursor), this.opts.reattachDelayMs ?? 50);
          return;
        }
        this.set({ phase: "lost", reason: `${f.reason}: ${f.message}`, leaseHeld: false });
        return;
      }
    }
  }

  /** The sink has finished the bytes up to `cursor`: tell the Core, one acknowledgement at a time, coalescing. */
  private consumed(cursor: bigint): void {
    if (cursor > this.ackTarget) this.ackTarget = cursor;
    void this.pump();
  }

  private async pump(): Promise<void> {
    if (this.ackBusy) return;
    this.ackBusy = true;
    try {
      while (!this.disposed && this.attachId !== null && this.ackTarget > this.ackSent) {
        const id = this.attachId;
        const target = this.ackTarget;
        try {
          await this.bridge.terminalAck(id, target.toString());
          if (id === this.attachId) this.ackSent = target;
          else break;
        } catch {
          break; // the attachment ended; the reattach resets the counters
        }
      }
    } finally {
      this.ackBusy = false;
    }
  }

  /** Keystrokes: the first one takes the input lease (the agent's input is refused while it is held), then writes in order. */
  input(data: string | Uint8Array): Promise<void> {
    const run = async () => {
      const id = this.attachId;
      if (!id || !this.running || this.state.phase === "exited") return;
      this.wantLease = true;
      if (!this.state.leaseHeld) {
        try {
          const r = await this.bridge.terminalInput(id, true);
          this.set({ leaseHeld: r.held });
          if (!r.held) return;
        } catch (e) {
          this.sink.notice(`input refused: ${(e as Error).message}`);
          return;
        }
      }
      try {
        await this.bridge.terminalWrite(id, data);
      } catch (e) {
        this.sink.notice(`input refused: ${(e as Error).message}`);
      }
    };
    this.chain = this.chain.then(run, run);
    return this.chain as Promise<void>;
  }

  /** The view lost focus: give the input lease back so the agent can type again. */
  release(): Promise<void> {
    this.wantLease = false;
    const run = async () => {
      const id = this.attachId;
      if (!id || !this.state.leaseHeld) return;
      try {
        await this.bridge.terminalInput(id, false);
      } catch {
        // the attachment ended: the lease went with it
      }
      this.set({ leaseHeld: false });
    };
    this.chain = this.chain.then(run, run);
    return this.chain as Promise<void>;
  }

  /** The viewer's size, sent to the PTY (the Core records it on the task). */
  async resize(rows: number, cols: number, force = false): Promise<void> {
    if (!force && this.size?.rows === rows && this.size.cols === cols) return;
    this.size = { rows, cols };
    const id = this.attachId;
    if (!id || !this.running || this.state.phase === "exited") return;
    try {
      await this.bridge.terminalResize(id, rows, cols);
      this.resizeRetries = 0;
    } catch (e) {
      this.sink.notice(`resize not applied yet: ${(e as Error).message}`);
      // The broker answers a resize in the order of the stream, so a terminal with a large backlog can take longer than the Core waits;
      // ask again (a few times at most) with the size the view has by then.
      if (this.resizeRetries++ < 3) {
        const retry = setTimeout(() => {
          if (!this.disposed && this.attachId === id && this.size) void this.resize(this.size.rows, this.size.cols, true);
        }, this.opts.resizeRetryMs ?? 2000);
        retry.unref?.();
      }
    }
  }

  /** Close the view: the process is untouched; the attachment and the lease end. */
  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    if (this.reattachTimer) clearTimeout(this.reattachTimer);
    this.off?.();
    this.off = null;
    const id = this.attachId;
    this.attachId = null;
    if (id) await this.bridge.terminalDetach(id).catch(() => undefined);
  }
}

/** A terminal's label for lists and headings. */
export function terminalLabel(t: TerminalViewJson): string {
  return t.title.trim() || `terminal ${t.terminalId.slice(0, 8)}`;
}

/** Who owns it, in words: agent-owned terminals say so, and say what typing does. */
export function ownerNote(t: TerminalViewJson): string {
  if (t.owner === "agent") return "Agent terminal. Typing here takes the input lease: the agent's own input to this terminal is refused while you hold it, until you leave the view.";
  return "Terminal started from this window.";
}

export function isRunning(t: TerminalViewJson): boolean {
  return t.state === "RUNNING";
}

/** A running timer from the Core's clock: whole seconds as m:ss. */
export function elapsedLabel(ms: string | number): string {
  const s = Math.floor(Number(ms) / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
