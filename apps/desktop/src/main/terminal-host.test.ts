import assert from "node:assert/strict";
import { test } from "node:test";
import type { CoreClient } from "@modbit/ide-adapter-core";
import type { TerminalFrame } from "@modbit/surface-protocol";
import { frameJson, TerminalHost, type TerminalFrameJson } from "./terminal-host.ts";

const SID = "s".repeat(32);
const TID = "a".repeat(32);

function out(attachId: string, cursor: number, text: string): TerminalFrame {
  return { attachId, sessionId: "t1", body: { case: "output", value: { cursor: BigInt(cursor), stream: "pty", data: new TextEncoder().encode(text) } } } as unknown as TerminalFrame;
}

function fake() {
  const calls: string[] = [];
  const client = {
    leaseGeneration: () => 1n,
    async attachTerminal(_s: string, _t: string, terminalId: string, after: bigint, o: { takeInputLease?: boolean }) {
      calls.push(`attach:${terminalId}:${after}:${o.takeInputLease === true}`);
      return { attachId: "att1", terminal: { sessionId: terminalId, taskId: { value: new Uint8Array(16).fill(0xaa) }, owner: "agent", argv: [], title: "x", state: "RUNNING", startedAtMs: 0n, elapsedMs: 0n, bytesSoFar: 0n, oldestCursor: 0n, replayWindowBytes: 0n, pty: true, rows: 24, cols: 80, inputLeaseHolder: "", outputRef: "", cwd: "", timedOut: false }, afterCursor: after, windowBytes: 4096n, stallMs: 0n, inputLeaseHeld: o.takeInputLease === true };
    },
    async ackTerminal(a: string, c: bigint) {
      calls.push(`ack:${a}:${c}`);
      return c;
    },
    async detachTerminal(a: string) {
      calls.push(`detach:${a}`);
      return { wasAttached: true, ackedCursor: 0n };
    },
    async setTerminalInput(_s: string, a: string, hold: boolean) {
      calls.push(`input:${a}:${hold}`);
      return { held: hold, holder: "user:1", offset: 1n };
    },
    async resizeTerminal(_s: string, a: string, rows: number, cols: number) {
      calls.push(`resize:${a}:${rows}x${cols}`);
      return { rows, cols, offset: 1n };
    },
    async writeTerminal(_s: string, a: string, d: Uint8Array) {
      calls.push(`write:${a}:${d.length}`);
      return { bytes: BigInt(d.length), cursor: 0n, offset: 1n };
    },
  } as unknown as CoreClient;
  const sent: TerminalFrameJson[] = [];
  const host = new TerminalHost(() => client, async () => undefined, (f) => sent.push(f));
  return { host, calls, sent };
}

test("PX-048: only frames of attachments this window opened reach the renderer", async () => {
  const { host, sent } = fake();
  host.onFrame(out("stranger", 0, "x"));
  await host.attach(SID, TID, "t1", 0n, { windowBytes: 0n, takeInputLease: false, stealInputLease: false });
  host.onFrame(out("att1", 0, "hello"));
  host.onFrame(out("other", 0, "no"));
  const f = sent.find((s) => s.kind === "output");
  assert.ok(f && f.kind === "output");
  assert.equal(new TextDecoder().decode(f.data), "hello");
  assert.equal(f.cursor, "0");
  assert.equal(sent.filter((s) => s.attachId !== "att1").length, 0);
});

test("PX-048: a frame that beats the attach's own registration is held and released, in order", async () => {
  const { host, sent } = fake();
  host.onFrame(out("att1", 0, "first"));
  host.onFrame(out("att1", 5, "second"));
  assert.equal(sent.length, 0, "nothing is sent for an id this window has not registered");
  await host.attach(SID, TID, "t1", 0n, { windowBytes: 0n, takeInputLease: false, stealInputLease: false });
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(sent.map((s) => (s.kind === "output" ? new TextDecoder().decode(s.data) : "")), ["first", "second"]);
});

test("PX-048: acknowledgements only move forward and only for attachments this window holds", async () => {
  const { host, calls } = fake();
  await host.attach(SID, TID, "t1", 100n, { windowBytes: 0n, takeInputLease: false, stealInputLease: false });
  await host.ack("att1", 150n);
  await host.ack("att1", 150n);
  await host.ack("att1", 120n);
  await host.ack("att1", 200n);
  assert.deepEqual(calls.filter((c) => c.startsWith("ack:")), ["ack:att1:150", "ack:att1:200"]);
  await assert.rejects(() => host.ack("nope", 1n), /UNKNOWN_ATTACHMENT/);
  await assert.rejects(() => host.write("nope", new Uint8Array(1)), /UNKNOWN_ATTACHMENT/);
  await assert.rejects(() => host.input("nope", true, false), /UNKNOWN_ATTACHMENT/);
  await assert.rejects(() => host.resize("nope", 1, 1), /UNKNOWN_ATTACHMENT/);
});

test("PX-048: a window that reloads detaches everything it opened; a lost Core connection tells the renderer to reattach", async () => {
  const { host, calls, sent } = fake();
  await host.attach(SID, TID, "t1", 0n, { windowBytes: 0n, takeInputLease: true, stealInputLease: false });
  assert.deepEqual(host.open().map((a) => a.attachId), ["att1"]);
  host.connectionLost();
  assert.equal(host.open().length, 0);
  const ended = sent.at(-1);
  assert.ok(ended && ended.kind === "ended" && ended.reason === "CORE_RESTARTED");
  await host.attach(SID, TID, "t1", 0n, { windowBytes: 0n, takeInputLease: false, stealInputLease: false });
  await host.detachAll();
  assert.ok(calls.includes("detach:att1"));
  assert.equal(host.open().length, 0);
});

test("PX-048: frames convert to the renderer's shape with cursors as decimal strings", () => {
  const exited = frameJson({ attachId: "a", sessionId: "t", body: { case: "exited", value: { exitCode: 2, durationMs: 9n, totalBytes: 77n, timedOut: false, cancelled: false } } } as unknown as TerminalFrame);
  assert.deepEqual(exited, { attachId: "a", terminalId: "t", kind: "exit", exitCode: 2, signal: null, durationMs: "9", totalBytes: "77", timedOut: false, cancelled: false });
  const ended = frameJson({ attachId: "a", sessionId: "t", body: { case: "ended", value: { reason: "SLOW_CONSUMER", message: "m", oldestCursor: 1n, resumeCursor: 2n } } } as unknown as TerminalFrame);
  assert.deepEqual(ended, { attachId: "a", terminalId: "t", kind: "ended", reason: "SLOW_CONSUMER", message: "m", oldestCursor: "1", resumeCursor: "2" });
  assert.equal(frameJson({ attachId: "a", sessionId: "t", body: { case: undefined } } as unknown as TerminalFrame), null);
});
