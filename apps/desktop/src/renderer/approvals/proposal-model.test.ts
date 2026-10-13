import { test } from "node:test";
import assert from "node:assert/strict";
import type { ModeProposalInfo, ModeProposalListInfo } from "../../shared/control-types.ts";
import { latestSettled, modeWords, pendingOf, proposalRefusalWords, proposalTitle, RESULT_VISIBLE_MS, secondsLeft, settledWords } from "./proposal-model.ts";

const proposal = (over: Partial<ModeProposalInfo> = {}): ModeProposalInfo => ({
  proposalId: "mp-call00",
  taskId: "t1",
  fromMode: "PLAN",
  toMode: "AGENT",
  reason: "the plan is ready",
  status: "PENDING",
  outcomeReason: "",
  proposedAtMs: 1_000,
  expiresAtMs: 16_000,
  decidedAtMs: 0,
  ...over,
});

const list = (proposals: ModeProposalInfo[], nowMs: number): ModeProposalListInfo => ({ taskId: "t1", proposals, nowMs });

test("REQ-PX-055: the card is titled from the Core's modes and the time left is read against the Core's clock", () => {
  assert.equal(proposalTitle(proposal()), "The agent suggests switching to Agent");
  assert.equal(modeWords("MULTITASK"), "Multitask");
  assert.equal(modeWords("SOMETHING_NEW"), "something_new");
  // This machine's clock is 100 s ahead of the Core's; the Core says 5 s of the window are gone.
  const skew = 6_000 - 106_000;
  assert.equal(secondsLeft(proposal(), 106_000, skew), 10);
  assert.equal(secondsLeft(proposal(), 200_000, skew), 0, "never negative");
});

test("REQ-PX-055: only a pending proposal is a card; the latest settled one is a note while it is recent", () => {
  const done = proposal({ proposalId: "mp-a", status: "SKIPPED", outcomeReason: "UNANSWERED_15S", decidedAtMs: 16_000 });
  const open = proposal({ proposalId: "mp-b" });
  assert.equal(pendingOf([done, open])?.proposalId, "mp-b");
  assert.equal(pendingOf([done]), null);
  assert.equal(latestSettled(list([done, open], 20_000))?.proposalId, "mp-a");
  assert.equal(latestSettled(list([done], 16_000 + RESULT_VISIBLE_MS + 1)), null, "an old note is not shown again");
  assert.equal(latestSettled(null), null);
});

test("REQ-PX-055: a skip is never worded as a switch; every typed reason says the mode is unchanged", () => {
  const skipped = settledWords(proposal({ status: "SKIPPED", outcomeReason: "UNANSWERED_15S" }));
  assert.match(skipped.headline, /Skipped/);
  assert.match(skipped.detail, /15 seconds/);
  assert.match(skipped.detail, /still Plan/);
  assert.match(settledWords(proposal({ status: "SKIPPED", outcomeReason: "CORE_RESTARTED" })).detail, /Core restarted/);
  assert.match(settledWords(proposal({ status: "SKIPPED", outcomeReason: "TASK_ENDED" })).detail, /task ended/);
  assert.match(settledWords(proposal({ status: "SKIPPED", outcomeReason: "MODE_CHANGED" })).headline, /Dropped/);
  assert.match(settledWords(proposal({ status: "SKIPPED", outcomeReason: "NEW_REASON" })).detail, /still Plan/);
  assert.match(settledWords(proposal({ status: "DECLINED", outcomeReason: "DECLINED_BY_USER" })).headline, /Kept Plan/);
  const accepted = settledWords(proposal({ status: "ACCEPTED", outcomeReason: "ACCEPTED_BY_USER" }));
  assert.equal(accepted.headline, "Switched to Agent.");
  assert.equal(accepted.tone, "ok");
  assert.equal(skipped.tone, "warn");
});

test("REQ-PX-055: a refused answer is worded from the Core's typed code", () => {
  assert.match(proposalRefusalWords("Error invoking remote method 'x': Error: PROPOSAL_NOT_PENDING: it ended"), /already ended/);
  assert.match(proposalRefusalWords("MODE_CHANGED: x"), /mode changed/);
  assert.match(proposalRefusalWords("UNKNOWN_PROPOSAL: x"), /does not know/);
  assert.match(proposalRefusalWords("BAD_ARGUMENT: proposalId must be a proposal id"), /^The Core refused: proposalId/);
});
