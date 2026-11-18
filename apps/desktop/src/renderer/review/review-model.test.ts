import { test } from "node:test";
import assert from "node:assert/strict";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";
import { changeSet, checksWords, findingsOf, reviewRows, stateWords } from "./review-model.ts";

const header = (taskId: string, over: Partial<AgentHeaderView> = {}): AgentHeaderView => ({ taskId, sessionId: "s", workspaceRoot: "/repos/modbit", title: `Task ${taskId}`, subtitle: "", createdAtMs: 0, updatedAtMs: 0, statusClass: "READY" as AgentHeaderView["statusClass"], statusLabel: "", unread: false, pendingApproval: false, pendingPlan: false, contextPercent: 0, filesChanged: 0, linesAdded: 0, linesRemoved: 0, lastCheckpointAtMs: 0, subagent: false, archived: false, executionLocation: "", origin: "", taskState: "Running", lastOffset: "0", readOffset: "0", attentionItems: 0, ...over });
const ack = (n: string): PullRequestAckView => ({ status: "OPENED", approvalId: "", intentHash: "", number: n, url: "", branch: "task/x", headSha: "", candidateRevision: "1", receipts: 1, replayed: false, detail: "" });
const bundle = (over: Partial<ReviewBundleView> = {}): ReviewBundleView => ({ taskId: "t", taskState: "ReadyForReview", workspaceRevision: "2", baseCommit: "", workspaceRoot: "", files: [], planJson: "", selfReviewJson: "", verificationRuns: [], attributions: [], quarantined: [], invariantFindings: [], receipts: 0, evidenceLinks: [], ciEvidence: [], ciRejected: [], repairAttempts: [], reviewComments: [], ...over });

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
    verificationRuns: [{ id: "v", stage: "BASELINE", status: "FAIL", candidateRevision: "2", checks: [{ id: "unit", status: "PASS" }, { id: "lint", status: "FAIL" }] }],
    invariantFindings: ["DI-3 EXTERNAL_SIDE_EFFECT src/a.ts: writes outside the task's folder"],
    selfReviewJson: JSON.stringify({ summary: "s", self_review: { findings: [{ text: "Tidied the helper", resolved: true }, { text: "The second leg is hidden", resolved: false }] } }),
    ciEvidence: [{ name: "build", runId: "1", status: "completed", conclusion: "failure", url: "", completedAt: "", logRef: "", logTruncated: false, commit: "abc", provenance: "ci" }],
  });
  const f = findingsOf(b);
  assert.deepEqual(f.map((x) => `${x.label}|${x.text}|${x.path}|${x.open}`), ["Check|lint failed||true", "Change rule|writes outside the task's folder|src/a.ts|true", "Self-review|The second leg is hidden||true", "Forge check|build failed on the forge||true", "Self-review|Tidied the helper||false"]);
  for (const x of f) assert.doesNotMatch(`${x.label} ${x.text}`, /DI-\d|EXTERNAL_SIDE_EFFECT|[A-Z]{2,}_[A-Z]/);
  assert.equal(checksWords(b), "1 of 2 checks passed");
  assert.equal(checksWords(bundle()), null);
  assert.deepEqual(findingsOf(bundle({ selfReviewJson: "not json" })), []);
});

test("REQ-PX-208: the change set counts each file's added and removed lines from the Core's hunks", () => {
  const h = (lines: string[]) => ({ index: 0, header: "@@", lines, toolCallId: "", toolName: "", turnId: "", runId: "", attributed: false, alsoCallIds: [], attributionBasis: "", hunkDigest: "", decision: "", autoAcceptEligible: false });
  const b = bundle({ files: [{ path: "notes.txt", status: "M", binary: false, fileRevision: "", oldContentRef: "", newContentRef: "", hunks: [h([" a", "-b", "+c", "+d"])] }, { path: "x.png", status: "A", binary: true, fileRevision: "", oldContentRef: "", newContentRef: "", hunks: [] }] });
  assert.deepEqual(changeSet(b), [{ path: "notes.txt", status: "changed", added: 2, removed: 1 }, { path: "x.png", status: "binary", added: 0, removed: 0 }]);
  assert.equal(stateWords("SomethingNew"), "Something new");
});
