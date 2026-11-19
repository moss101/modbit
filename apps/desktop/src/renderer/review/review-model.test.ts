import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import type { ReviewerFindingView, ReviewerResultView } from "../../shared/review-findings-types.ts";
import { changeSet, checksWords, findingPlace, findingRefusal, findingsOf, fixedWords, handledWords, openReviewerFindings, reviewerWords, reviewRows, severityWords, stateWords, verifiedWords, whoWords } from "./review-model.ts";

const header = (taskId: string, over: Partial<AgentHeaderView> = {}): AgentHeaderView => ({ taskId, sessionId: "s", workspaceRoot: "/repos/modbit", title: `Task ${taskId}`, subtitle: "", createdAtMs: 0, updatedAtMs: 0, statusClass: "READY" as AgentHeaderView["statusClass"], statusLabel: "", unread: false, pendingApproval: false, pendingPlan: false, contextPercent: 0, filesChanged: 0, linesAdded: 0, linesRemoved: 0, lastCheckpointAtMs: 0, subagent: false, archived: false, executionLocation: "", origin: "", taskState: "Running", lastOffset: "0", readOffset: "0", attentionItems: 0, ...over });
const ack = (n: string): PullRequestAckView => ({ status: "OPENED", approvalId: "", intentHash: "", number: n, url: "", branch: "task/x", headSha: "", candidateRevision: "1", receipts: 1, replayed: false, detail: "" });
const bundle = (over: Partial<ReviewBundleView> = {}): ReviewBundleView => ({ taskId: "t", taskState: "ReadyForReview", workspaceRevision: "2", baseCommit: "", workspaceRoot: "", files: [], planJson: "", selfReviewJson: "", verificationRuns: [], attributions: [], quarantined: [], invariantFindings: [], receipts: 0, evidenceLinks: [], ciEvidence: [], ciRejected: [], repairAttempts: [], reviewComments: [], reviewer: { state: "", verdict: "", confidence: 0, summary: "", candidateRevision: "0", resultDigest: "", findings: [], questions: [], note: "" }, ...over });

test("REQ-PX-208: tasks with changes split into pull requests this window knows of and local changes, newest first", () => {
  const hs = [header("a", { filesChanged: 2, linesAdded: 5, linesRemoved: 1, updatedAtMs: 1 }), header("b", { taskState: "ReadyForReview", updatedAtMs: 3, checkoutRoot: "/repos/modbit" }), header("c"), header("d", { filesChanged: 1, archived: true }), header("e", { filesChanged: 1, updatedAtMs: 2 })];
  const { pulls, local } = reviewRows(hs, (id) => (id === "e" ? ack("7") : null));
  assert.deepEqual(pulls.map((r) => r.taskId), ["e"]);
  assert.deepEqual(local.map((r) => r.taskId), ["b", "a"]);
  assert.equal(local[0]!.where, "its own worktree");
  assert.equal(local[1]!.where, "modbit");
  assert.equal(local[0]!.state, "Ready for review");
});

test("REQ-PX-208: findings are what the bundle carries, in words, open first; no raw code is shown", () => {
  const b = bundle({
    verificationRuns: [
      { id: "v0", stage: "BASELINE", status: "FAIL", candidateRevision: "1", checks: [{ id: "configured_command:old", status: "FAIL" }] },
      { id: "v", stage: "COMPLETION", status: "FAIL", candidateRevision: "2", checks: [{ id: "configured_command:pnpm test", status: "PASS" }, { id: "adv:boundary-pinned", status: "SKIP" }, { id: "configured_command:lint", status: "FAIL" }] },
    ],
    invariantFindings: ["DI-3 EXTERNAL_SIDE_EFFECT src/a.ts: writes outside the task's folder"],
    selfReviewJson: JSON.stringify({ summary: "s", self_review: { findings: [{ text: "Tidied the helper", resolved: true }, { text: "The second leg is hidden", resolved: false }] } }),
    ciEvidence: [{ name: "build", runId: "1", status: "completed", conclusion: "failure", url: "", completedAt: "", logRef: "", logTruncated: false, commit: "abc", provenance: "ci" }],
  });
  const f = findingsOf(b);
  assert.deepEqual(f.map((x) => `${x.label}|${x.text}|${x.path}|${x.open}`), ["Check|lint failed||true", "Change rule|writes outside the task's folder|src/a.ts|true", "Self-review|The second leg is hidden||true", "Forge check|build failed on the forge||true", "Self-review|Tidied the helper||false"]);
  for (const x of f) assert.doesNotMatch(`${x.label} ${x.text}`, /DI-\d|EXTERNAL_SIDE_EFFECT|[A-Z]{2,}_[A-Z]|configured_command|adv:/);
  assert.equal(checksWords(b), "1 of 3 checks passed, 1 skipped");
  assert.equal(checksWords(bundle({ verificationRuns: [{ id: "v", stage: "COMPLETION", status: "PASSED", candidateRevision: "2", checks: [{ id: "a", status: "PASS" }, { id: "adv:x", status: "SKIP" }] }] })), "1 check passed, 1 skipped");
  assert.equal(checksWords(bundle()), null);
  assert.deepEqual(findingsOf(bundle({ selfReviewJson: "not json" })), []);
});

test("REQ-PX-208: the change set counts each file's added and removed lines from the Core's hunks", () => {
  const h = (lines: string[]) => ({ index: 0, header: "@@", lines, toolCallId: "", toolName: "", turnId: "", runId: "", attributed: false, alsoCallIds: [], attributionBasis: "", hunkDigest: "", decision: "", autoAcceptEligible: false });
  const b = bundle({ files: [{ path: "notes.txt", status: "M", binary: false, fileRevision: "", oldContentRef: "", newContentRef: "", hunks: [h([" a", "-b", "+c", "+d"])] }, { path: "x.png", status: "A", binary: true, fileRevision: "", oldContentRef: "", newContentRef: "", hunks: [] }] });
  assert.deepEqual(changeSet(b), [{ path: "notes.txt", status: "changed", added: 2, removed: 1 }, { path: "x.png", status: "binary", added: 0, removed: 0 }]);
  assert.equal(stateWords("SomethingNew"), "Something new");
});

const finding = (over: Partial<ReviewerFindingView> = {}): ReviewerFindingView => ({ findingId: "f1", severity: "HIGH", category: "security", path: "src/a.ts", line: 3, claim: "the check is too weak", suggestedAction: "compare a hash", verified: true, unsupportedReason: "", dismissed: false, dismissedBy: "", dismissedReason: "", fixRequests: 0, ...over });
const reviewer = (over: Partial<ReviewerResultView> = {}): ReviewerResultView => ({ state: "CURRENT", verdict: "BLOCK", confidence: 77, summary: "weak check", candidateRevision: "7", resultDigest: "c".repeat(64), findings: [], questions: [], note: "", ...over });

test("REQ-PX-214: the reviewer's verdict is in plain words, a stale report says so, an unreadable one shows nothing of itself", () => {
  assert.equal(reviewerWords(reviewer({ state: "" })), null);
  const w = reviewerWords(reviewer())!;
  assert.equal(w.headline, "The reviewer says this change should not be accepted as it is.");
  assert.equal(w.detail, "weak check The reviewer was 77 percent sure.");
  assert.equal(w.stale, false);
  assert.equal(reviewerWords(reviewer({ state: "STALE", verdict: "PASS", confidence: 0, summary: "" }))!.headline, "The reviewer found nothing to change.");
  assert.equal(reviewerWords(reviewer({ state: "STALE" }))!.stale, true);
  const u = reviewerWords(reviewer({ state: "UNREADABLE", verdict: "", summary: "", confidence: 0, note: "the stored result does not hash" }))!;
  assert.deepEqual([u.unreadable, u.detail], [true, "the stored result does not hash"]);
  assert.equal(reviewerWords(reviewer({ verdict: "MAYBE" }))!.headline, "The reviewer gave an answer this page has no words for.");
  for (const x of ["PASS", "REVISE", "BLOCK", "UNCERTAIN"]) assert.doesNotMatch(reviewerWords(reviewer({ verdict: x }))!.headline, /[A-Z]{3,}/);
});

test("REQ-PX-214: a finding names its place, says whether the Core found it, and what became of it, in words", () => {
  assert.equal(findingPlace(finding()), "src/a.ts, line 3");
  assert.equal(findingPlace(finding({ line: 0 })), "src/a.ts");
  assert.equal(findingPlace(finding({ path: "" })), "");
  assert.equal(severityWords("CRITICAL"), "Critical");
  assert.equal(severityWords(""), "Unrated");
  assert.equal(verifiedWords(finding()), "The Core found this file in the code.");
  const unverified = finding({ verified: false, unsupportedReason: "`src/ghost.ts` is not in the tree at the candidate revision" });
  assert.match(verifiedWords(unverified), /^The Core could not confirm this: `src\/ghost\.ts` is not in the tree/);
  assert.doesNotMatch(verifiedWords(unverified), /found this file/, "an unconfirmed finding is never worded as confirmed");
  assert.equal(handledWords(finding()), "");
  assert.equal(handledWords(finding({ fixRequests: 1 })), "You asked the agent to fix this one.");
  assert.equal(handledWords(finding({ fixRequests: 2 })), "You asked the agent to fix this one 2 times.");
  assert.equal(handledWords(finding({ dismissed: true, dismissedBy: "user:abc", dismissedReason: "not part of this change" })), "Set aside by you: not part of this change");
  assert.equal(whoWords("agent:solver"), "solver");
  assert.equal(whoWords(""), "someone");
});

test("REQ-PX-214: Walk through goes over the findings no one has set aside, in the reviewer's order", () => {
  const r = reviewer({ findings: [finding({ findingId: "a" }), finding({ findingId: "b", dismissed: true }), finding({ findingId: "c" })] });
  assert.deepEqual(openReviewerFindings(r).map((f) => f.findingId), ["a", "c"]);
});

test("REQ-PX-214: the Core's refusals of a command on a finding are in words; a fix says when the agent takes it up", () => {
  assert.match(findingRefusal("Error invoking remote method 'review:fixFinding': Error: RESULT_CHANGED: the reviewer's latest result is not the one"), /reported again/);
  assert.match(findingRefusal("FINDING_DISMISSED: finding `f1` was dismissed"), /already set aside/);
  assert.match(findingRefusal("RESULT_UNREADABLE: the stored reviewer result is missing"), /cannot be read back safely/);
  assert.doesNotMatch(findingRefusal("UNKNOWN_FINDING: x"), /UNKNOWN_FINDING/);
  assert.match(fixedWords("Waiting(UserInput)"), /when you resume the task/);
  assert.match(fixedWords("Running"), /next step/);
});
