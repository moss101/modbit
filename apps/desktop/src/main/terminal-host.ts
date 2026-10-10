/**
 * The terminal bridge of the apps panel (REQ-PX-048, PX-043, docs/21, docs/81):
 * Electron main relays one Core terminal stream per attachment to the renderer
 * and the renderer's acknowledgements, keystrokes and resizes back. It owns no
 * terminal state: the Core's broker holds the PTY, the replay window and the
 * person's input lease; main keeps only which attachments this window opened,
 * so a reload or a Core restart can end them.
 *
 * Backpressure is the Core's: it sends at most the attach's window beyond the
 * last acknowledgement, and this host acknowledges to the Core only what the
 * renderer says it has consumed, so a renderer that stops consuming stops the
 * stream and nothing here grows with it.
 */
import type { CoreClient } from "@modbit/ide-adapter-core";
import type { TerminalFrame, TerminalView } from "@modbit/surface-protocol";

const MAX_EARLY_FRAMES = 512;
const MAX_EARLY_ATTACHMENTS = 16;
const toHex =(b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

export interface TerminalViewJson {
  terminalId: string;
  taskId: string;
  owner: string;
  title: string;
  state: string;
  exitCode: number | null;
  startedAtMs: number;
  elapsedMs: string;
  bytesSoFar: string;
  oldestCursor: string;
  replayWindowBytes: string;
  pty: boolean;
  rows: number;
  cols: number;
  inputLeaseHolder: string;
  cwd: string;
  timedOut: boolean;
}

export function viewJson(t: TerminalView): TerminalViewJson {
  return {
    terminalId: t.sessionId,
    taskId: t.taskId ? toHex(t.taskId.value) : "",
    owner: t.owner,
    title: t.title,
    state: t.state,
    exitCode: t.exitCode ?? null,
    startedAtMs: Number(t.startedAtMs),
    elapsedMs: t.elapsedMs.toString(),
    bytesSoFar: t.bytesSoFar.toString(),
    oldestCursor: t.oldestCursor.toString(),
    replayWindowBytes: t.replayWindowBytes.toString(),
    pty: t.pty,
    rows: t.rows,
    cols: t.cols,
    inputLeaseHolder: t.inputLeaseHolder,
    cwd: t.cwd,
    timedOut: t.timedOut,
  };
}

/** One frame as the renderer receives it. Cursors are decimal strings (64-bit on the wire). */
export type TerminalFrameJson =
  | { attachId: string; terminalId: string; kind: "output"; cursor: string; stream: string; data: Uint8Array }
  | { attachId: string; terminalId: string; kind: "exit"; exitCode: number | null; signal: number | null; durationMs: string; totalBytes: string; timedOut: boolean; cancelled: boolean }
  | { attachId: string; terminalId: string; kind: "ended"; reason: string; message: string; oldestCursor: string; resumeCursor: string }
  | { attachId: string; terminalId: string; kind: "lease"; held: boolean; holder: string };

export function frameJson(f: TerminalFrame): TerminalFrameJson | null {
  const base = { attachId: f.attachId, terminalId: f.sessionId };
  switch (f.body.case) {
    case "output":
      return { ...base, kind: "output", cursor: f.body.value.cursor.toString(), stream: f.body.value.stream, data: f.body.value.data };
    case "exited": {
      const x = f.body.value;
      return { ...base, kind: "exit", exitCode: x.exitCode ?? null, signal: x.signal ?? null, durationMs: x.durationMs.toString(), totalBytes: x.totalBytes.toString(), timedOut: x.timedOut, cancelled: x.cancelled };
    }
    case "ended":
      return { ...base, kind: "ended", reason: f.body.value.reason, message: f.body.value.message, oldestCursor: f.body.value.oldestCursor.toString(), resumeCursor: f.body.value.resumeCursor.toString() };
    case "lease":
      return { ...base, kind: "lease", held: f.body.value.held, holder: f.body.value.holder };
    default:
      return null;
  }
}

export interface Attachment {
  attachId: string;
  terminalId: string;
  taskId: string;
  sessionId: string;
}

export class TerminalHost {
  private readonly attachments = new Map<string, Attachment>();
  /** The highest cursor acknowledged to the Core per attachment: acknowledgements never go backwards. */
  private readonly acked = new Map<string, bigint>();
  private readonly early = new Map<string, TerminalFrame[]>();

  private readonly client: () => CoreClient;
  private readonly ensureLease: (c: CoreClient, sessionId: string) => Promise<void>;
  private readonly send: (frame: TerminalFrameJson) => void;

  constructor(client: () => CoreClient, ensureLease: (c: CoreClient, sessionId: string) => Promise<void>, send: (frame: TerminalFrameJson) => void) {
    this.client = client;
    this.ensureLease = ensureLease;
    this.send = send;
  }

  /** The attachments open now (for tests and the window-reload cleanup). */
  open(): Attachment[] {
    return [...this.attachments.values()];
  }

  async list(taskId?: string): Promise<{ terminals: TerminalViewJson[]; nowMs: number; defaultWindowBytes: string; stallMs: string }> {
    const l = await this.client().listTerminals(taskId);
    return { terminals: l.terminals.map(viewJson), nowMs: Number(l.nowMs), defaultWindowBytes: l.defaultWindowBytes.toString(), stallMs: l.stallMs.toString() };
  }

  async attach(sessionId: string, taskId: string, terminalId: string, afterCursor: bigint, opts: { windowBytes: bigint; takeInputLease: boolean; stealInputLease: boolean }): Promise<{ attachId: string; terminal: TerminalViewJson; afterCursor: string; windowBytes: string; inputLeaseHeld: boolean }> {
    const c = this.client();
    if (opts.takeInputLease) await this.ensureLease(c, sessionId);
    const a = await c.attachTerminal(sessionId, taskId, terminalId, afterCursor, { windowBytes: opts.windowBytes, takeInputLease: opts.takeInputLease, stealInputLease: opts.stealInputLease });
    this.attachments.set(a.attachId, { attachId: a.attachId, terminalId, taskId, sessionId });
    this.acked.set(a.attachId, afterCursor);
    if (!a.terminal) throw new Error("CORE_PROTOCOL: the attach answered without the terminal");
    // Release what arrived before the id was registered, after the renderer has the attach's answer (it queues frames until then).
    const waiting = this.early.get(a.attachId) ?? [];
    this.early.delete(a.attachId);
    setImmediate(() => {
      for (const f of waiting) this.onFrame(f);
    });
    return { attachId: a.attachId, terminal: viewJson(a.terminal), afterCursor: a.afterCursor.toString(), windowBytes: a.windowBytes.toString(), inputLeaseHeld: a.inputLeaseHeld };
  }

  /**
   * Frames of this window's attachments go to the renderer. The Core can send a
   * first frame in the same read as the attach's own answer, before `attach`
   * has registered the id, so frames of an id not known yet wait (bounded: the
   * Core's window already bounds what one attachment sends) and are released
   * when the attach registers. A frame of an attachment that never registers is
   * dropped when the wait is full.
   */
  onFrame(f: TerminalFrame): void {
    if (!this.attachments.has(f.attachId)) {
      const waiting = this.early.get(f.attachId) ?? [];
      if (waiting.length < MAX_EARLY_FRAMES) waiting.push(f);
      this.early.set(f.attachId, waiting);
      if (this.early.size > MAX_EARLY_ATTACHMENTS) this.early.delete(this.early.keys().next().value as string);
      return;
    }
    const j = frameJson(f);
    if (j) this.send(j);
  }

  async ack(attachId: string, cursor: bigint): Promise<string> {
    this.owned(attachId);
    const prev = this.acked.get(attachId) ?? 0n;
    if (cursor <= prev) return prev.toString();
    this.acked.set(attachId, cursor);
    return (await this.client().ackTerminal(attachId, cursor)).toString();
  }

  async detach(attachId: string): Promise<{ wasAttached: boolean; ackedCursor: string }> {
    this.owned(attachId);
    this.attachments.delete(attachId);
    this.acked.delete(attachId);
    const d = await this.client().detachTerminal(attachId);
    return { wasAttached: d.wasAttached, ackedCursor: d.ackedCursor.toString() };
  }

  async input(attachId: string, hold: boolean, steal: boolean): Promise<{ held: boolean; holder: string; offset: string }> {
    const a = this.owned(attachId);
    const c = this.client();
    await this.ensureLease(c, a.sessionId);
    const r = await c.setTerminalInput(a.sessionId, attachId, hold, steal);
    return { held: r.held, holder: r.holder, offset: r.offset.toString() };
  }

  async resize(attachId: string, rows: number, cols: number): Promise<{ rows: number; cols: number }> {
    const a = this.owned(attachId);
    const c = this.client();
    await this.ensureLease(c, a.sessionId);
    const r = await c.resizeTerminal(a.sessionId, attachId, rows, cols);
    return { rows: r.rows, cols: r.cols };
  }

  async write(attachId: string, data: Uint8Array): Promise<{ bytes: string; cursor: string }> {
    const a = this.owned(attachId);
    const c = this.client();
    await this.ensureLease(c, a.sessionId);
    const r = await c.writeTerminal(a.sessionId, attachId, data);
    return { bytes: r.bytes.toString(), cursor: r.cursor.toString() };
  }

  async kill(sessionId: string, taskId: string, terminalId: string): Promise<{ outcome: string; exitCode: number | null }> {
    const c = this.client();
    await this.ensureLease(c, sessionId);
    const k = await c.killTerminal(sessionId, taskId, terminalId, "stopped from the apps panel");
    return { outcome: k.outcome, exitCode: k.exitCode ?? null };
  }

  /** The window went away (reload, crash): its attachments are over. Best effort on the Core. */
  async detachAll(): Promise<void> {
    const open = this.open();
    this.attachments.clear();
    this.acked.clear();
    await Promise.all(open.map((a) => this.client().detachTerminal(a.attachId).catch(() => undefined)));
  }

  /** The Core connection was replaced: the old attachments died with it; tell the renderer to reattach from its cursors. */
  connectionLost(): void {
    const open = this.open();
    this.attachments.clear();
    this.acked.clear();
    for (const a of open) this.send({ attachId: a.attachId, terminalId: a.terminalId, kind: "ended", reason: "CORE_RESTARTED", message: "the Core connection was replaced", oldestCursor: "0", resumeCursor: "0" });
  }

  private owned(attachId: string): Attachment {
    const a = this.attachments.get(attachId);
    if (!a) throw new Error("UNKNOWN_ATTACHMENT: this window holds no such terminal attachment");
    return a;
  }
}
