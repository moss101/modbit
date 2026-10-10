import assert from "node:assert/strict";
import { test } from "node:test";
import type { TerminalFrameJson, TerminalViewJson } from "../../preload/preload.ts";
import { elapsedLabel, ownerNote, TerminalController, type TerminalBridge, type TerminalSink } from "./terminal-controller.ts";

const VIEW: TerminalViewJson = { terminalId: "t1", taskId: "a".repeat(32), owner: "agent", title: "sh -c x", state: "RUNNING", exitCode: null, startedAtMs: 0, elapsedMs: "0", bytesSoFar: "0", oldestCursor: "0", replayWindowBytes: "0", pty: true, rows: 24, cols: 80, inputLeaseHolder: "", cwd: "", timedOut: false };
const enc = (s: string) => new TextEncoder().encode(s);
const out = (attachId: string, cursor: number, text: string): TerminalFrameJson => ({ attachId, terminalId: "t1", kind: "output", cursor: String(cursor), stream: "pty", data: enc(text) });
const tick = () => new Promise<void>((r) => setImmediate(r));

class FakeBridge implements TerminalBridge {
  listeners = new Set<(f: TerminalFrameJson) => void>();
  calls: string[] = [];
  acks: string[] = [];
  writes: string[] = [];
  attaches: { after: string; lease: boolean }[] = [];
  nextAttach = 0;
  attachError: Error | null = null;
  attachDelay: Promise<void> | null = null;
  leaseHeld = false;
  processState = "RUNNING";
  resizeFailures = 0;
  onAttach: ((id: string) => void) | null = null;
  async terminalAttach(_s: string, _t: string, _term: string, after: string, opts?: { takeInputLease?: boolean }) {
    this.attaches.push({ after, lease: opts?.takeInputLease === true });
    if (this.attachDelay) await this.attachDelay;
    if (this.attachError) {
      const e = this.attachError;
      this.attachError = null;
      throw e;
    }
    const attachId = `att${++this.nextAttach}`;
    this.onAttach?.(attachId);
    return { attachId, terminal: { ...VIEW, state: this.processState }, afterCursor: after, windowBytes: "262144", inputLeaseHeld: opts?.takeInputLease === true };
  }
  async terminalAck(attachId: string, cursor: string) {
    this.acks.push(`${attachId}:${cursor}`);
    return cursor;
  }
  async terminalDetach(attachId: string) {
    this.calls.push(`detach:${attachId}`);
    return { wasAttached: true, ackedCursor: "0" };
  }
  async terminalInput(attachId: string, hold: boolean) {
    this.calls.push(`input:${attachId}:${hold}`);
    this.leaseHeld = hold;
    return { held: hold, holder: "user:1", offset: "1" };
  }
  async terminalResize(attachId: string, rows: number, cols: number) {
    if (this.resizeFailures > 0) {
      this.resizeFailures--;
      this.calls.push(`resize-failed:${attachId}`);
      throw new Error("BROKER_UNAVAILABLE: the broker did not answer within 10s");
    }
    this.calls.push(`resize:${attachId}:${rows}x${cols}`);
    return { rows, cols };
  }
  async terminalWrite(attachId: string, data: Uint8Array | string) {
    this.writes.push(typeof data === "string" ? data : new TextDecoder().decode(data));
    this.calls.push(`write:${attachId}`);
    return { bytes: "1", cursor: "0" };
  }
  onTerminalFrame(cb: (f: TerminalFrameJson) => void) {
    this.listeners.add(cb);
    return () => void this.listeners.delete(cb);
  }
  emit(f: TerminalFrameJson) {
    for (const l of [...this.listeners]) l(f);
  }
}

class FakeSink implements TerminalSink {
  text = "";
  resets = 0;
  notices: string[] = [];
  pending: (() => void)[] = [];
  /** When false the sink consumes at once; when true it holds each write until released (a slow renderer). */
  slow = false;
  write(data: Uint8Array, done: () => void) {
    this.text += new TextDecoder().decode(data);
    if (this.slow) this.pending.push(done);
    else done();
  }
  reset() {
    this.resets++;
    this.text = "";
  }
  notice(t: string) {
    this.notices.push(t);
  }
  release() {
    for (const d of this.pending.splice(0)) d();
  }
}

function make(opts: { reattachDelayMs?: number; resizeRetryMs?: number } = {}) {
  const bridge = new FakeBridge();
  const sink = new FakeSink();
  const states: string[] = [];
  const c = new TerminalController(bridge, sink, { sessionId: "s".repeat(32), taskId: VIEW.taskId, terminalId: "t1" }, (s) => states.push(s.phase), { reattachDelayMs: 0, ...opts });
  return { bridge, sink, c, states };
}

test("PX-048: the view attaches from the cursor it is given, shows output in order and acknowledges what the sink consumed", async () => {
  const { bridge, sink, c } = make();
  await c.start(0n);
  assert.deepEqual(bridge.attaches, [{ after: "0", lease: false }]);
  bridge.emit(out("att1", 0, "hello "));
  bridge.emit(out("att1", 6, "world\r\n"));
  await tick();
  assert.equal(sink.text, "hello world\r\n");
  assert.equal(c.snapshot().cursor, 13n);
  assert.equal(bridge.acks.at(-1), "att1:13");
  assert.equal(c.snapshot().phase, "live");
});

test("PX-048: a slow consumer is not acknowledged past what it has consumed, so the Core stops sending (bounded memory)", async () => {
  const { bridge, sink, c } = make();
  sink.slow = true;
  await c.start(0n);
  bridge.emit(out("att1", 0, "aaaa"));
  bridge.emit(out("att1", 4, "bbbb"));
  await tick();
  assert.deepEqual(bridge.acks, [], "nothing consumed, nothing acknowledged");
  sink.release();
  await tick();
  assert.equal(bridge.acks.at(-1), "att1:8");
  // Many chunks consumed in one go acknowledge together: one request in flight at a time.
  sink.slow = true;
  for (let i = 0; i < 50; i++) bridge.emit(out("att1", 8 + i, "x"));
  sink.release();
  await tick();
  assert.equal(bridge.acks.at(-1), "att1:58");
  assert.ok(bridge.acks.length <= 4, `acknowledgements coalesce: ${bridge.acks.length}`);
});

test("PX-048: frames that arrive before the attach answers are kept and applied in order", async () => {
  const { bridge, sink, c } = make();
  bridge.onAttach = (id) => {
    bridge.emit(out(id, 0, "early "));
    bridge.emit(out("someone-else", 0, "NOT MINE"));
  };
  await c.start(0n);
  bridge.emit(out("att1", 6, "late"));
  await tick();
  assert.equal(sink.text, "early late");
});

test("PX-048: a replay that overlaps what is on screen does not repeat bytes (byte-exact)", async () => {
  const { bridge, sink, c } = make();
  await c.start(0n);
  bridge.emit(out("att1", 0, "abcdef"));
  bridge.emit(out("att1", 3, "defghi"));
  bridge.emit(out("att1", 0, "abc"));
  await tick();
  assert.equal(sink.text, "abcdefghi");
  assert.equal(c.snapshot().bytes, 9n);
});

test("PX-048: a stream that ends without the process ending reattaches from what is on screen, not from the Core's guess", async () => {
  const { bridge, sink, c, states } = make();
  await c.start(100n);
  bridge.emit(out("att1", 100, "12345"));
  await tick();
  bridge.emit({ attachId: "att1", terminalId: "t1", kind: "ended", reason: "SLOW_CONSUMER", message: "no ack", oldestCursor: "0", resumeCursor: "999" });
  await new Promise((r) => setTimeout(r, 20));
  assert.deepEqual(bridge.attaches.map((a) => a.after), ["100", "105"]);
  bridge.emit(out("att2", 105, "6"));
  await tick();
  assert.equal(sink.text, "123456");
  assert.ok(states.includes("attaching"));
  assert.equal(c.snapshot().phase, "live");
});

test("PX-048: a cursor older than the replay window shows what is still replayable and says what was dropped", async () => {
  const { bridge, sink, c } = make();
  bridge.attachError = new Error("CURSOR_EXPIRED: cursor 0 is older than the replay window; oldest_cursor=4096");
  await c.start(0n);
  assert.deepEqual(bridge.attaches.map((a) => a.after), ["0", "4096"]);
  assert.match(sink.notices.join("\n"), /no longer replayable; showing from byte 4096/);
  assert.equal(c.snapshot().phase, "live");
});

test("PX-048: a refused attach is shown with the Core's reason, never retried blindly", async () => {
  const { bridge, c } = make();
  bridge.attachError = new Error("SESSION_NOT_OWNED: terminal t1 does not belong to task x");
  await c.start(0n);
  assert.equal(c.snapshot().phase, "refused");
  assert.match(c.snapshot().reason ?? "", /SESSION_NOT_OWNED/);
  assert.equal(bridge.attaches.length, 1);
});

test("PX-048: the exit frame ends input and is shown with its code", async () => {
  const { bridge, sink, c } = make();
  await c.start(0n);
  bridge.emit(out("att1", 0, "bye"));
  bridge.emit({ attachId: "att1", terminalId: "t1", kind: "exit", exitCode: 3, signal: null, durationMs: "5", totalBytes: "3", timedOut: false, cancelled: false });
  await tick();
  assert.equal(c.snapshot().phase, "exited");
  assert.equal(c.snapshot().exit?.exitCode, 3);
  await c.input("x");
  assert.deepEqual(bridge.writes, [], "no keystroke goes to a process that ended");
  assert.equal(sink.text, "bye");
});

test("PX-048: typing takes the input lease once, writes in order, and blur gives the lease back", async () => {
  const { bridge, c } = make();
  await c.start(0n);
  void c.input("l");
  void c.input("s");
  await c.input("\r");
  assert.deepEqual(bridge.writes, ["l", "s", "\r"]);
  assert.deepEqual(bridge.calls.filter((x) => x.startsWith("input:")), ["input:att1:true"], "the lease is taken by the first keystroke only");
  assert.equal(c.snapshot().leaseHeld, true);
  await c.release();
  assert.equal(bridge.calls.at(-1), "input:att1:false");
  assert.equal(c.snapshot().leaseHeld, false);
  // Typing again takes it again.
  await c.input("x");
  assert.equal(bridge.calls.filter((x) => x === "input:att1:true").length, 2);
});

test("PX-048: a refused lease is reported and nothing is typed", async () => {
  const { bridge, sink, c } = make();
  bridge.terminalInput = async () => {
    throw new Error("LEASE_HELD: another viewer holds the input");
  };
  await c.start(0n);
  await c.input("a");
  assert.deepEqual(bridge.writes, []);
  assert.match(sink.notices.join(), /input refused: LEASE_HELD/);
});

test("PX-048: a view that held the lease asks for it again when it reattaches", async () => {
  const { bridge, c } = make();
  await c.start(0n);
  await c.input("a");
  bridge.emit({ attachId: "att1", terminalId: "t1", kind: "ended", reason: "CORE_RESTARTED", message: "", oldestCursor: "0", resumeCursor: "0" });
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(bridge.attaches[1]?.lease, true);
  assert.equal(c.snapshot().leaseHeld, true);
  await c.release();
  bridge.emit({ attachId: "att2", terminalId: "t1", kind: "ended", reason: "BROKER_LOST", message: "", oldestCursor: "0", resumeCursor: "0" });
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(bridge.attaches[2]?.lease, false, "after a blur the lease is not taken back");
});

test("PX-048: the viewer's size reaches the terminal, once per change, and again after a reattach", async () => {
  const { bridge, c } = make();
  await c.start(0n);
  await c.resize(30, 100);
  await c.resize(30, 100);
  await c.resize(40, 120);
  assert.deepEqual(bridge.calls.filter((x) => x.startsWith("resize:")), ["resize:att1:30x100", "resize:att1:40x120"]);
  bridge.emit({ attachId: "att1", terminalId: "t1", kind: "ended", reason: "BROKER_ERROR", message: "", oldestCursor: "0", resumeCursor: "0" });
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(bridge.calls.at(-1), "resize:att2:40x120");
});

test("PX-048: an ended stream the view cannot resume is shown as lost; dispose detaches and stops listening", async () => {
  const { bridge, c } = make();
  await c.start(0n);
  bridge.emit({ attachId: "att1", terminalId: "t1", kind: "ended", reason: "SOMETHING_ELSE", message: "gone", oldestCursor: "0", resumeCursor: "0" });
  assert.equal(c.snapshot().phase, "lost");
  const second = make();
  await second.c.start(0n);
  await second.c.dispose();
  assert.ok(second.bridge.calls.includes("detach:att1"));
  assert.equal(second.bridge.listeners.size, 0);
});

test("PX-048: labels say who owns a terminal and what typing does", () => {
  assert.match(ownerNote(VIEW), /Agent terminal\. Typing here takes the input lease/);
  assert.equal(elapsedLabel("65000"), "1:05");
});

test("PX-048: a terminal whose process already ended takes neither a resize nor a keystroke (the Core would wait on a session that is gone)", async () => {
  const { bridge, c } = make();
  bridge.processState = "EXITED";
  await c.start(0n);
  await c.resize(30, 100);
  await c.input("x");
  assert.deepEqual(bridge.calls.filter((x) => x.startsWith("resize") || x.startsWith("write") || x.startsWith("input")), []);
});

test("PX-048: a resize the broker could not answer in time is asked again, a few times at most", async () => {
  const { bridge, c } = make({ resizeRetryMs: 5 });
  await c.start(0n);
  bridge.resizeFailures = 2;
  await c.resize(30, 100);
  await new Promise((r) => setTimeout(r, 80));
  assert.deepEqual(bridge.calls.filter((x) => x.startsWith("resize")), ["resize-failed:att1", "resize-failed:att1", "resize:att1:30x100"]);
  const second = make({ resizeRetryMs: 5 });
  await second.c.start(0n);
  second.bridge.resizeFailures = 99;
  await second.c.resize(10, 10);
  await new Promise((r) => setTimeout(r, 150));
  assert.equal(second.bridge.calls.filter((x) => x.startsWith("resize-failed")).length, 4, "the first ask and three retries");
});
