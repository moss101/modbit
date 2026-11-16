/**
 * REQ-PX-206: Inbox's record of updates. Entries are made from the Core's headers only, hold ids, the kind, a title, a time and
 * an offset (never transcript text), are capped, survive a reload of the file, and take their read state from the Core's read
 * offset. Retained mutations: a header carrying the assistant's last line, and a file that carries text beside an entry.
 */
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { cleanEntry, ENTRY_KEYS, kindOf, notificationTarget, TITLE_MAX, UPDATES_CAP, UpdatesRecord, withReadState, type HeaderFacts } from "./inbox-record.ts";

const T1 = "a".repeat(32);
const T2 = "b".repeat(32);
const file = () => join(mkdtempSync(join(tmpdir(), "modbit-inbox-record-")), "inbox-updates.json");
const header = (taskId: string, statusClass: string, extra: Partial<HeaderFacts> = {}): HeaderFacts => ({ taskId, title: "Fix the parser", statusClass, subagent: false, origin: "desktop", lastOffset: "40", readOffset: "10", updatedAtMs: 1_700_000_000_000, ...extra });

test("kinds come from the Core's status class: ready for review, a finished child, an automation's run, a failure; nothing else", () => {
  assert.equal(kindOf(header(T1, "READY_FOR_REVIEW_UNSEEN")), "READY_FOR_REVIEW");
  assert.equal(kindOf(header(T1, "READY_FOR_REVIEW_SEEN")), "READY_FOR_REVIEW");
  assert.equal(kindOf(header(T1, "COMPLETED", { subagent: true })), "CHILD_FINISHED");
  assert.equal(kindOf(header(T1, "READY_FOR_REVIEW_UNSEEN", { origin: "automation" })), "AUTOMATION_RAN");
  assert.equal(kindOf(header(T1, "COMPLETED", { origin: "automation" })), "AUTOMATION_RAN");
  assert.equal(kindOf(header(T1, "FAILED")), "FAILED");
  for (const c of ["RUNNING", "WAITING", "NEEDS_ATTENTION", "DRAFT", "ARCHIVED", "COMPLETED"]) assert.equal(kindOf(header(T1, c)), null, c);
});

test("a state is noticed once; leaving it and coming back is a new notice; the record is reloaded from its file", () => {
  const f = file();
  const r = new UpdatesRecord(f);
  assert.equal(r.observe([header(T1, "RUNNING")]), false);
  assert.equal(r.observe([header(T1, "READY_FOR_REVIEW_UNSEEN")]), true);
  assert.equal(r.observe([header(T1, "READY_FOR_REVIEW_SEEN")]), false, "seen is the same state");
  r.save();
  const again = new UpdatesRecord(f);
  assert.equal(again.entries().length, 1);
  assert.equal(again.observe([header(T1, "READY_FOR_REVIEW_UNSEEN")]), false, "a restart does not notice the same state twice");
  again.observe([header(T1, "RUNNING", { lastOffset: "50" })]);
  again.observe([header(T1, "READY_FOR_REVIEW_UNSEEN", { lastOffset: "60" })]);
  assert.deepEqual(again.entries().map((e) => e.offset), ["40", "60"]);
});

test("the record's shape: exactly ids, kind, title, time and offset; the assistant's words never reach it (retained mutation)", () => {
  const f = file();
  const r = new UpdatesRecord(f);
  const said = "I rewrote the parser and the tests pass";
  // A header the Core extends with the assistant's last line (PX-218): the record takes none of it.
  r.observe([{ ...header(T1, "FAILED"), lastLine: said } as HeaderFacts]);
  r.save();
  const onDisk = JSON.parse(readFileSync(f, "utf8")) as { entries: Record<string, unknown>[] };
  assert.equal(onDisk.entries.length, 1);
  assert.deepEqual(Object.keys(onDisk.entries[0]!).sort(), [...ENTRY_KEYS].sort());
  assert.ok(!readFileSync(f, "utf8").includes(said));
  // A file that carries text beside an entry (written by something else): the text is dropped when it is read back.
  const bad = { ...onDisk.entries[0]!, text: said, reason: said };
  writeFileSync(f, JSON.stringify({ version: 1, entries: [bad] }));
  const reread = new UpdatesRecord(f);
  assert.deepEqual(Object.keys(reread.entries()[0]!).sort(), [...ENTRY_KEYS].sort());
  reread.save();
  assert.ok(!readFileSync(f, "utf8").includes(said));
  // Entries that are not entries are dropped; a long title is cut.
  assert.equal(cleanEntry({ ...bad, taskId: "../../etc" }), null);
  assert.equal(cleanEntry({ ...bad, kind: "TRANSCRIPT" }), null);
  assert.equal(cleanEntry({ ...bad, offset: "1e9" }), null);
  assert.equal([...cleanEntry({ ...bad, title: "x".repeat(500) })!.title].length, TITLE_MAX);
});

test("the cap holds: the oldest go first", () => {
  const f = file();
  const r = new UpdatesRecord(f, 3);
  for (let i = 0; i < 5; i++) {
    const t = i.toString(16).padStart(32, "0");
    r.observe([header(t, "FAILED", { lastOffset: String(10 + i), updatedAtMs: 1000 + i })]);
  }
  assert.deepEqual(r.entries().map((e) => e.offset), ["12", "13", "14"]);
  assert.equal(UPDATES_CAP, 200);
  r.save();
  assert.equal(new UpdatesRecord(f, 3).entries().length, 3);
});

test("read state is the Core's read offset for the task; a task the Core no longer lists has nothing to read", () => {
  const r = new UpdatesRecord(file());
  r.observe([header(T1, "FAILED", { lastOffset: "40", updatedAtMs: 1 }), header(T2, "READY_FOR_REVIEW_UNSEEN", { lastOffset: "70", updatedAtMs: 2 })]);
  const view = withReadState(r.entries(), [
    { taskId: T1, readOffset: "40" },
    { taskId: T2, readOffset: "69" },
  ]);
  assert.deepEqual(view.map((e) => [e.taskId, e.read]), [[T2, false], [T1, true]], "newest first");
  assert.equal(withReadState(r.entries(), [])[0]!.read, true);
});

test("clearing empties the record and a cleared notice does not come back", () => {
  const f = file();
  const r = new UpdatesRecord(f);
  r.observe([header(T1, "FAILED")]);
  assert.equal(r.clear(), 1);
  r.save();
  const again = new UpdatesRecord(f);
  assert.equal(again.entries().length, 0);
  assert.equal(again.observe([header(T1, "FAILED")]), false);
});

test("a notification's click opens the task its id names, else Inbox", () => {
  assert.deepEqual(notificationTarget(`task:${T1}`), { taskId: T1 });
  assert.deepEqual(notificationTarget("burst"), { taskId: null });
  assert.deepEqual(notificationTarget(`task:${T1}/../x`), { taskId: null });
});
