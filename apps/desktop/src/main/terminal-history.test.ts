import assert from "node:assert/strict";
import { test } from "node:test";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import type { CoreClient } from "@modbit/ide-adapter-core";
import { ObjectRangeChunkSchema, ReadObjectRangeSchema } from "@modbit/surface-protocol";
import { MAX_HISTORY_BYTES, planHistoryRange, TerminalHost } from "./terminal-host.ts";

const TID = "a".repeat(32);
const REF = "f".repeat(64);

/** A sealed object of `total` bytes (byte i is i % 251) behind a terminal that ended at `head`. */
function sealed(opts: { head: bigint; total: number; outputRef: string; listed?: boolean; oldest?: bigint }) {
  const reads: { offset: bigint; length: bigint }[] = [];
  const client = {
    async listTerminals() {
      return { terminals: opts.listed === false ? [] : [{ sessionId: "t1", taskId: { value: new Uint8Array(16).fill(0xaa) }, bytesSoFar: opts.head, oldestCursor: opts.oldest ?? opts.head - BigInt(opts.total), outputRef: opts.outputRef }] };
    },
    async command(name: string, payload: Uint8Array) {
      assert.equal(name, "ReadObjectRange");
      const p = fromBinary(ReadObjectRangeSchema, payload);
      assert.equal(p.objectHash, REF, "the hash is the Core's own, never the caller's");
      reads.push({ offset: p.offset, length: p.length });
      const end = Math.min(opts.total, Number(p.offset + p.length));
      const data = new Uint8Array(Math.max(0, end - Number(p.offset)));
      for (let i = 0; i < data.length; i++) data[i] = (Number(p.offset) + i) % 251;
      return { result: toBinary(ObjectRangeChunkSchema, create(ObjectRangeChunkSchema, { data, totalBytes: BigInt(opts.total) })) };
    },
  } as unknown as CoreClient;
  return { host: new TerminalHost(() => client, async () => undefined, () => undefined), reads };
}

test("PX-048: planHistoryRange clamps to what the Core retained and to one page", () => {
  assert.deepEqual(planHistoryRange(100n, 1000n, 0n, 100n), { from: 100n, to: 100n }, "wholly before the start: empty");
  assert.deepEqual(planHistoryRange(100n, 1000n, 50n, 300n), { from: 100n, to: 300n }, "straddling the start: clamped");
  assert.deepEqual(planHistoryRange(0n, 1000n, 900n, 5000n), { from: 900n, to: 1000n }, "past the head: clamped");
  const big = planHistoryRange(0n, 10n ** 9n, 0n, 10n ** 9n);
  assert.equal(big.to - big.from, BigInt(MAX_HISTORY_BYTES), "a read is a page, never the object");
});

test("PX-048: history reads the range from the terminal's own sealed output, translating cursors to object offsets", async () => {
  const { host, reads } = sealed({ head: 5000n, total: 3000, outputRef: REF });
  // The object holds cursors 2000..5000.
  const r = await host.history(TID, "t1", 2500n, 2600n);
  assert.equal(r.startCursor, "2000");
  assert.equal(r.from, "2500");
  assert.equal(r.exhausted, false);
  assert.equal(r.data.length, 100);
  assert.equal(r.data[0], 500 % 251, "cursor 2500 is object offset 500");
  assert.deepEqual(reads.at(-1), { offset: 500n, length: 100n });
  const edge = await host.history(TID, "t1", 1900n, 2100n);
  assert.equal(edge.from, "2000");
  assert.equal(edge.data.length, 100);
  assert.equal(edge.exhausted, true, "a range that reaches the start says nothing is older");
  const before = reads.length;
  const gone = await host.history(TID, "t1", 0n, 1000n);
  assert.equal(gone.data.length, 0);
  assert.equal(gone.exhausted, true);
  assert.equal(reads.length, before + 1, "only the probe of the object's size was read for a range that is gone");
});

test("PX-048: bytes the log gained after the object was sealed do not shift the object's cursors", async () => {
  // The object holds cursors 2000..5000; the listing's head is 5003 (three bytes a PTY added after the end).
  const { host } = sealed({ head: 5000n, total: 3000, outputRef: REF });
  const late = sealed({ head: 5003n, total: 3000, outputRef: REF, oldest: 2000n });
  const a = await host.history(TID, "t1", 2500n, 2600n);
  const b = await late.host.history(TID, "t1", 2500n, 2600n);
  assert.deepEqual(Array.from(b.data), Array.from(a.data), "the same cursors read the same bytes");
  const past = await late.host.history(TID, "t1", 4990n, 5010n);
  assert.equal(past.data.length, 10, "a range stops at the object's end");
  const short = sealed({ head: 4000n, total: 3000, outputRef: REF, oldest: 2000n });
  await assert.rejects(short.host.history(TID, "t1", 2500n, 2600n), /OBJECT_LENGTH_MISMATCH/);
});

test("PX-048: history of a running terminal, or of one this task does not own, is refused", async () => {
  const running = sealed({ head: 10n, total: 10, outputRef: "" });
  await assert.rejects(running.host.history(TID, "t1", 0n, 5n), /NO_OUTPUT_REF/);
  const foreign = sealed({ head: 10n, total: 10, outputRef: REF, listed: false });
  await assert.rejects(foreign.host.history(TID, "t1", 0n, 5n), /UNKNOWN_TERMINAL/);
  assert.equal(foreign.reads.length, 0, "nothing was read for a terminal the task does not own");
});
