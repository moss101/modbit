import assert from "node:assert/strict";
import { test } from "node:test";
import { HistoryLoader, historyText, type HistoryReader } from "./terminal-history.ts";

/** A sealed output of `total` bytes whose byte at cursor c is the digit c % 10, retained from `start`. */
function reader(start: number, total: number, log: string[] = []): HistoryReader {
  return async (from, to) => {
    log.push(`${from}..${to}`);
    const f = from < BigInt(start) ? start : Number(from);
    const t = Math.min(Number(to), total);
    const data = new Uint8Array(Math.max(0, t - f));
    for (let i = 0; i < data.length; i++) data[i] = 0x30 + ((f + i) % 10);
    return { startCursor: BigInt(start), from: BigInt(f), data };
  };
}

test("PX-048: older ranges are read newest first, joined in cursor order, once, and end at what the Core kept", async () => {
  const log: string[] = [];
  const l = new HistoryLoader(reader(100, 1000, log), 1000n, () => undefined, 300);
  assert.equal(await l.loadOlder(), "loaded");
  assert.equal(await l.loadOlder(), "loaded");
  assert.equal(await l.loadOlder(), "exhausted", "the third range reaches byte 100, the first one the Core retained");
  assert.equal(await l.loadOlder(), "exhausted");
  assert.deepEqual(log, ["700..1000", "400..700", "100..400"], "no range is asked for twice, and none past the start");
  const b = l.bytes();
  assert.equal(b.length, 900);
  for (let i = 0; i < b.length; i++) assert.equal(b[i], 0x30 + ((100 + i) % 10), `byte ${100 + i} is in its place`);
  const s = l.snapshot();
  assert.equal(s.loadedFrom, 100n);
  assert.equal(s.exhausted, true);
  assert.equal(s.pages, 3);
});

test("PX-048: a range that straddles the start of what the Core kept is clamped and ends the history", async () => {
  const l = new HistoryLoader(reader(250, 1000), 1000n, () => undefined, 300);
  assert.equal(await l.loadOlder(), "loaded");
  assert.equal(await l.loadOlder(), "loaded");
  assert.equal(await l.loadOlder(), "exhausted", "400..700 then 250..400 (asked 100..400, clamped)");
  assert.equal(l.snapshot().loadedFrom, 250n);
  assert.equal(l.bytes().length, 750);
});

test("PX-048: two requests while one is in flight are one read", async () => {
  const log: string[] = [];
  const inner = reader(0, 1000, log);
  let release!: () => void;
  const hold = new Promise<void>((r) => (release = r));
  const l = new HistoryLoader(async (f, t) => (await hold, inner(f, t)), 1000n, () => undefined, 300);
  const a = l.loadOlder();
  const b = l.loadOlder();
  assert.equal(l.snapshot().loading, true);
  release();
  assert.deepEqual(await Promise.all([a, b]), ["loaded", "loaded"]);
  assert.deepEqual(log, ["700..1000"]);
  assert.equal(l.snapshot().pages, 1);
});

test("PX-048: a range that does not end where the loaded text begins is refused, not placed", async () => {
  const l = new HistoryLoader(async () => ({ startCursor: 0n, from: 10n, data: new Uint8Array(5) }), 1000n, () => undefined, 300);
  assert.equal(await l.loadOlder(), "failed");
  assert.match(l.snapshot().error ?? "", /HISTORY_RANGE_MISMATCH/);
  assert.equal(l.snapshot().pages, 0);
  assert.equal(l.snapshot().exhausted, false);
});

test("PX-048: a Core refusal is shown and the next request asks the same range again", async () => {
  let fail = true;
  const inner = reader(0, 1000);
  const log: string[] = [];
  const l = new HistoryLoader(
    async (f, t) => {
      log.push(`${f}..${t}`);
      if (fail) throw new Error("NO_OUTPUT_REF: the process is still running");
      return inner(f, t);
    },
    1000n,
    () => undefined,
    300,
  );
  assert.equal(await l.loadOlder(), "failed");
  assert.match(l.snapshot().error ?? "", /NO_OUTPUT_REF/);
  fail = false;
  assert.equal(await l.loadOlder(), "loaded");
  assert.deepEqual(log, ["700..1000", "700..1000"]);
  assert.equal(l.snapshot().error, null);
});

test("PX-048: a view that starts at the beginning of the output has nothing older", async () => {
  const l = new HistoryLoader(reader(0, 10), 0n, () => undefined);
  assert.equal(l.snapshot().exhausted, true);
  assert.equal(await l.loadOlder(), "exhausted");
});

test("PX-048: history text drops terminal control sequences and normalises line ends", () => {
  const raw = new TextEncoder().encode("\u001b[31mred\u001b[0m\r\nnext\u001b]0;title\u0007 line\r\n");
  assert.equal(historyText(raw), "red\nnext line\n");
});

test("PX-048: a doubled CR before a LF is one line end, and a CR that ends the bytes is half of a line end that continues in the view", () => {
  assert.equal(historyText(new TextEncoder().encode("a\r\r\nb\r\nc\r")), "a\nb\nc");
  assert.equal(historyText(new TextEncoder().encode("a\rb")), "a\nb");
});
