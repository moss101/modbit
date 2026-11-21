import { test } from "node:test";
import assert from "node:assert/strict";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import type { PullRequestStateView } from "../../shared/conversation-types.ts";
import { canOpenRequest, checkWords, commentWords, forgeErrorWords, refusedWords, mergeArchiveWatcher, requestStateLine, requestStateWords, requestWords, autoFixWords } from "./pull-request-model.ts";

const ack = (over: Partial<PullRequestAckView>): PullRequestAckView => ({ status: "OPENED", approvalId: "a".repeat(32), intentHash: "b".repeat(64), number: "7", url: "https://github.test/o/r/pull/7", branch: "modbit/task-1", headSha: "c".repeat(40), candidateRevision: "3", receipts: 2, replayed: false, detail: "", ...over });
type Check = ReviewBundleView["ciEvidence"][number];
type Comment = ReviewBundleView["reviewComments"][number];
const check = (over: Partial<Check>): Check => ({ name: "ci", runId: "501", status: "completed", conclusion: "success", url: "", completedAt: "", logRef: "", logTruncated: false, commit: "d".repeat(40), provenance: "ci", ...over });
const comment = (over: Partial<Comment>): Comment => ({ commentId: "1", kind: "conversation", author: "reviewer", url: "", path: "", line: "0", body: "", trust: "UNTRUSTED_EXTERNAL_CONTENT", disposition: "STEERED", reason: "", inputId: "", answered: false, reportedBack: false, createdAt: "", ...over });

test("REQ-PX-202: the pane says what the Core answered about the request, and nothing about a request it did not answer for", () => {
  assert.equal(requestWords(null), null);
  for (const status of ["APPROVAL_PENDING", "DENIED", "FAILED", ""]) assert.equal(requestWords(ack({ status })), null, status);
  assert.deepEqual(requestWords(ack({})), { title: "Opened as #7", where: "https://github.test/o/r/pull/7", head: `modbit/task-1 · head ${"c".repeat(12)}`, receipts: "2 receipts" });
  assert.deepEqual(requestWords(ack({ status: "UPDATED", receipts: 1, replayed: true, branch: "" })), { title: "Updated as #7", where: "https://github.test/o/r/pull/7", head: `head ${"c".repeat(12)}`, receipts: "1 receipt, answered from the record" });
  // A state that was not read is never shown: none of the words is a state of the request on the forge.
  const all = Object.values(requestWords(ack({}))!).join(" ");
  assert.doesNotMatch(all, /\b(open|merged|closed|draft|approved|mergeable)\b/i);
});

test("REQ-PX-202: a request can be opened once the task is ready for review, and the reason is said before", () => {
  assert.equal(canOpenRequest("ReadyForReview"), true);
  assert.equal(canOpenRequest("Completed"), true);
  for (const s of ["Running", "Waiting", "Queued", "Failed", ""]) assert.equal(typeof canOpenRequest(s), "string", s);
});

test("REQ-PX-202: a check is the forge's own result for the pushed commit; a refused run says why", () => {
  assert.deepEqual(checkWords(check({})), { name: "ci", result: "success", tone: "ok", detail: `run 501 · ${"d".repeat(12)}` });
  assert.equal(checkWords(check({ conclusion: "failure" })).tone, "bad");
  assert.deepEqual([checkWords(check({ conclusion: "", status: "in_progress" })).result, checkWords(check({ conclusion: "", status: "in_progress" })).tone], ["in progress", "none"], "a run with no conclusion yet is neither passed nor failed");
  assert.equal(checkWords(check({ conclusion: "neutral" })).tone, "none");
  assert.equal(refusedWords({ name: "ci-previous", headSha: "e".repeat(40), reason: "HEAD_SHA_MISMATCH" }), `ci-previous: not counted, head sha mismatch (run on ${"e".repeat(12)})`);
});

test("REQ-PX-202: a comment says who wrote it and what became of it", () => {
  assert.deepEqual(commentWords(comment({})), { author: "@reviewer", where: "", outcome: "passed to the agent, not answered yet" });
  assert.equal(commentWords(comment({ answered: true, reportedBack: true })).outcome, "passed to the agent, which has answered, reported back on the request");
  assert.deepEqual(commentWords(comment({ disposition: "IGNORED", reason: "DISALLOWED_AUTHOR", author: "stranger", path: "src/a.rs", line: "14" })), { author: "@stranger", where: "src/a.rs:14", outcome: "ignored: disallowed author" });
});

test("REQ-PX-202: the Core's refusal to read the forge is said in words with its code", () => {
  assert.match(forgeErrorWords("Error invoking remote method 'review:ingestCi': Error: NO_PULL_REQUEST: the task has no pull request"), /^The Core has no pull request for this task.*\(NO_PULL_REQUEST\)\.$/);
  assert.equal(forgeErrorWords("Error invoking remote method 'review:ingestCi': Error: FORGE_UNAVAILABLE"), "FORGE_UNAVAILABLE");
});

const state = (over: Partial<PullRequestStateView>): PullRequestStateView => ({ owner: "o", repo: "r", number: 7, url: "https://github.test/o/r/pull/7", state: "OPEN", draft: false, checks: "PASSING", checksTotal: 2, checksPassing: 2, checksFailing: 0, checksPending: 0, failingChecks: [], headSha: "f".repeat(40), pushedSha: "f".repeat(40), readAtMs: 1_700_000_000_000, trigger: "REQUEST", readOffset: "9", openedOffset: "5", autoFix: false, autoFixAttempts: 0, autoFixCap: 3, autoFixStopped: false, autoFixLastCause: "", autoFixLastOffset: "0", ...over });

test("REQ-PX-211: a request the Core has not read has a number and no state", () => {
  assert.equal(requestStateWords(undefined), null);
  assert.equal(requestStateWords(null), null);
  const w = requestStateWords(state({ state: "", checks: "", readAtMs: 0, trigger: "", readOffset: "0" }))!;
  assert.deepEqual([w.number, w.state, w.tone, w.checks, w.read], ["#7", "not read yet", "unread", "", false]);
  assert.equal(requestStateLine(state({ state: "", checks: "", readAtMs: 0 })), "pull request #7, not read yet");
  // A state with no time of reading is not a read.
  assert.equal(requestStateWords(state({ readAtMs: 0 }))!.read, false);
});

test("REQ-PX-211: the state and checks are worded as the Core read them", () => {
  assert.equal(requestStateLine(state({})), "pull request #7, open, all 2 checks passing");
  assert.equal(requestStateLine(state({ state: "MERGED" })), "pull request #7, merged, all 2 checks passing");
  assert.equal(requestStateLine(state({ state: "CLOSED", checks: "NONE", checksTotal: 0 })), "pull request #7, closed, no checks");
  assert.equal(requestStateWords(state({ draft: true }))!.tone, "draft");
  const failing = requestStateWords(state({ checks: "FAILING", checksPassing: 1, checksFailing: 1, failingChecks: ["lint"] }))!;
  assert.deepEqual([failing.checks, failing.checksTone, failing.failing], ["1 of 2 checks failing", "bad", "lint"]);
  const pending = requestStateWords(state({ checks: "PENDING", checksTotal: 1, checksPassing: 0, checksPending: 1 }))!;
  assert.deepEqual([pending.checks, pending.checksTone], ["1 of 1 check running", "pending"]);
  assert.equal(requestStateWords(state({ checks: "UNAVAILABLE" }))!.checks, "checks could not be read");
  assert.equal(requestStateWords(state({ trigger: "SCHEDULE" }))!.by, "by the Core's schedule");
  assert.equal(requestStateWords(state({ checks: "PASSING", checksTotal: 1 }))!.checks, "the check passing");
});

test("REQ-PX-211: the Core's archive after merge is the archive right after the read that found the merge, on the same task", () => {
  const w = mergeArchiveWatcher();
  const ev = (taskId: string, eventType: string, payload: unknown = null) => ({ taskId, eventType, payload });
  assert.equal(w(ev("a", "ForgePullRequestStateRead", { state: "MERGED" })), null);
  assert.equal(w(ev("b", "ConversationArchived")), null, "another task's archive is not this one's");
  assert.equal(w(ev("a", "ConversationArchived")), "a");
  // An archive after an open read, or after anything else, is a person's.
  assert.equal(w(ev("c", "ForgePullRequestStateRead", { state: "OPEN" })), null);
  assert.equal(w(ev("c", "ConversationArchived")), null);
  assert.equal(w(ev("d", "ForgePullRequestStateRead", { state: "MERGED" })), null);
  assert.equal(w(ev("d", "ConversationRead")), null);
  assert.equal(w(ev("d", "ConversationArchived")), null);
  assert.equal(w({ eventType: "ConversationArchived" }), null);
  assert.equal(w(null), null);
});

test("REQ-PX-212: auto-fix words come from the Core's record: off says nothing, on counts against the cap, a stop says so", () => {
  assert.deepEqual(autoFixWords(state({})), { on: false, status: "", tone: "none" });
  assert.deepEqual(autoFixWords(null), { on: false, status: "", tone: "none" });
  assert.equal(autoFixWords(state({ autoFix: true })).status, "On. No repair turn yet; at most 3.");
  assert.equal(autoFixWords(state({ autoFix: true, autoFixAttempts: 1, autoFixLastCause: "CHECKS_FAILING" })).status, "1 of 3 repair turns used; the latest for failing checks.");
  assert.equal(autoFixWords(state({ autoFix: true, autoFixAttempts: 2, autoFixLastCause: "REVIEW_COMMENT" })).status, "2 of 3 repair turns used; the latest for a review comment.");
  const stopped = autoFixWords(state({ autoFix: true, autoFixAttempts: 3, autoFixStopped: true, autoFixLastCause: "CHECKS_FAILING" }));
  assert.equal(stopped.tone, "stopped");
  assert.equal(stopped.status, "Stopped after 3 of 3 repair turns. Turn it off and on again to allow more.");
  // A stop recorded while it was on says nothing once it is off.
  assert.deepEqual(autoFixWords(state({ autoFix: false, autoFixStopped: true })), { on: false, status: "", tone: "none" });
});
