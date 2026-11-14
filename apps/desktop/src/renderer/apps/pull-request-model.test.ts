import { test } from "node:test";
import assert from "node:assert/strict";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import { canOpenRequest, checkWords, commentWords, forgeErrorWords, refusedWords, requestWords } from "./pull-request-model.ts";

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
