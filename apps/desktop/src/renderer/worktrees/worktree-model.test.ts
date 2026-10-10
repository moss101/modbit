import { test } from "node:test";
import assert from "node:assert/strict";
import type { WorktreeInfo } from "../../shared/project-types.ts";
import { ageWords, groupByRepository, reportWords, sizeWords, worktreeStates } from "./worktree-model.ts";

let n = 0;
const w = (over: Partial<WorktreeInfo> = {}): WorktreeInfo => {
  n++;
  return { worktreeId: `wt${n}`, taskId: `t${n}`, kind: "TASK", path: `/data/worktrees/${n}`, originRoot: "/w/alpha", branch: `modbit/${n}`, baseRevision: "abc", state: "ACTIVE", disposition: "", dirty: false, unapplied: false, changedFiles: 0, bytes: "2048", createdAtMs: 1000 * n, lastActivityMs: 1000 * n, taskRunning: false, orphan: false, protected: false, removable: false, removableReason: "", setupStatus: "", setupDetail: "", neutralized: [], ...over };
};

test("a worktree's states come from the Core's own flags, in one place", () => {
  assert.deepEqual(worktreeStates(w({ taskRunning: true })).map((s) => s.id), ["running"]);
  assert.deepEqual(worktreeStates(w({ dirty: true, unapplied: true, changedFiles: 2 })).map((s) => s.id), ["dirty", "unapplied"]);
  assert.deepEqual(worktreeStates(w({ orphan: true, unapplied: true })).map((s) => s.id), ["unapplied", "orphan"]);
  assert.deepEqual(worktreeStates(w({ removable: true, removableReason: "its result was applied" })).map((s) => [s.id, s.detail]), [["removable", "its result was applied"]]);
  assert.deepEqual(worktreeStates(w({ removable: false, removableReason: "younger than the protection window", protected: true })).map((s) => [s.id, s.detail]), [["retained", "younger than the protection window"]]);
  assert.deepEqual(worktreeStates(w({ state: "MISSING", dirty: true })).map((s) => s.id), ["missing"]);
  // A discarded result leaves a dirty tree the Core calls removable: both are said.
  assert.deepEqual(worktreeStates(w({ dirty: true, removable: true, removableReason: "its result was discarded" })).map((s) => s.id), ["dirty", "removable"]);
  // A task waiting for the person's review is said so; the Core's flag (not ended) is unchanged.
  assert.deepEqual(worktreeStates(w({ taskRunning: true, dirty: true, unapplied: true }), "ReadyForReview").map((s) => s.id), ["review", "dirty", "unapplied"]);
  assert.deepEqual(worktreeStates(w({ taskRunning: true }), "Running").map((s) => s.id), ["running"]);
  // Words always accompany the state: no state is colour alone.
  for (const s of worktreeStates(w({ taskRunning: true, dirty: true }))) assert.ok(s.label.length > 0);
});

test("sizes and ages read as short words", () => {
  assert.equal(sizeWords("0"), "0 B");
  assert.equal(sizeWords("1536"), "1.5 KB");
  assert.equal(sizeWords(String(50 * 1024 * 1024 * 1024)), "50 GB");
  assert.equal(sizeWords("not a number"), "unknown size");
  const now = 10_000_000;
  assert.equal(ageWords(now - 5_000, now), "just now");
  assert.equal(ageWords(now - 5 * 60_000, now), "5 min");
  assert.equal(ageWords(now - 3 * 3_600_000, now), "3 h");
  assert.equal(ageWords(now - 3 * 86_400_000 - 1, now), "3 days");
  assert.equal(ageWords(0, now), "unknown age");
});

test("worktrees group by source repository, newest first, and those with no source come last", () => {
  const a1 = w({ originRoot: "/w/alpha", createdAtMs: 10 });
  const a2 = w({ originRoot: "/w/alpha", createdAtMs: 30 });
  const b1 = w({ originRoot: "/w/beta", createdAtMs: 20 });
  const orphan = w({ originRoot: "", orphan: true });
  const g = groupByRepository([orphan, a1, b1, a2]);
  assert.deepEqual(g.map((x) => x.label), ["alpha", "beta", "No source repository"]);
  assert.deepEqual(g[0]!.items.map((i) => i.worktreeId), [a2.worktreeId, a1.worktreeId]);
});

test("a cleanup that did not happen is Skipped, never Completed", () => {
  const base = { scanned: 4, removed: 1, bytesFreed: "2048", skippedReason: "", holderOwner: "" };
  assert.equal(reportWords({ ...base, status: "COMPLETED" }), "Completed: 1 removed of 4 looked at, 2.0 KB freed");
  assert.equal(reportWords({ ...base, status: "DRY_RUN" }), "Dry run: 1 removed of 4 looked at, 2.0 KB freed (nothing was removed)");
  assert.equal(reportWords({ ...base, status: "SKIPPED", skippedReason: "the cleanup lease is held" }), "Skipped: the cleanup lease is held");
  assert.match(reportWords({ ...base, status: "SKIPPED", holderOwner: "core-1" }), /^Skipped: /);
});
