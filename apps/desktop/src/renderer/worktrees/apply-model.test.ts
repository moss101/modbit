import { test } from "node:test";
import assert from "node:assert/strict";
import type { ApplyAckInfo, ApplyConflictInfo, ApplyOptionInfo } from "../../shared/project-types.ts";
import { DEFAULT_OPTION, canRemember, doneWords, missingPaths, optionsOf, requiredPaths, stepOf, typedPaths } from "./apply-model.ts";

const opt = (option: string, over: Partial<ApplyOptionInfo> = {}): ApplyOptionInfo => ({ option, destructive: false, needsConfirmation: false, confirmPaths: [], available: true, whyUnavailable: "", ...over });
const conflict = (over: Partial<ApplyConflictInfo> = {}): ApplyConflictInfo => ({ planDigest: "d".repeat(64), candidateTree: "t", checkoutHead: "h", candidateRevision: "3", paths: [], conflictingPaths: ["a.txt"], protectedPaths: [], dirtyPaths: [], options: [opt("MERGE_MANUALLY"), opt("CANCEL"), opt("OVERWRITE", { destructive: true, needsConfirmation: true, confirmPaths: ["a.txt"] })], defaultOption: "CANCEL", rememberedOption: "", ...over });
const ack = (over: Partial<ApplyAckInfo> = {}): ApplyAckInfo => ({ status: "APPLIED", applyId: "", approvalId: "", intentHash: "", planDigest: "", candidateRevision: "3", candidateTree: "", checkoutHeadBefore: "", conflict: null, appliedPaths: [], unresolvedPaths: [], effectReceiptIds: [], detail: "", code: "", option: "", stashRef: "", replayed: false, divergentPaths: [], ...over });

test("the Core's answer decides the step: conflict, approval, result or a stop that changed nothing", () => {
  assert.equal(stepOf(ack({ status: "APPROVAL_PENDING", approvalId: "x" }), "apply", {}).kind, "approve");
  assert.equal(stepOf(ack({ status: "CONFLICT", conflict: conflict() }), "apply", {}).kind, "conflict");
  assert.equal(stepOf(ack({ status: "APPLIED" }), "apply", {}).kind, "done");
  assert.equal(stepOf(ack({ status: "UNDONE" }), "undo", {}).kind, "done");
  assert.equal(stepOf(ack({ status: "NOTHING_TO_APPLY" }), "apply", {}).kind, "done");
  for (const status of ["STALE", "DENIED", "CANCELLED", "UNDO_CONFLICT"]) {
    const s = stepOf(ack({ status }), "apply", {});
    assert.equal(s.kind, "stopped");
    if (s.kind === "stopped") assert.equal(s.reason, status);
  }
  // A conflict with no view, or a status this window does not know, is a stop, never a guess.
  const odd = stepOf(ack({ status: "CONFLICT", conflict: null }), "apply", {});
  assert.equal(odd.kind, "stopped");
  assert.equal(stepOf(ack({ status: "SOMETHING_NEW" }), "apply", {}).kind, "stopped");
});

test("a refused overwrite returns to the choices with the Core's reason", () => {
  const s = stepOf(ack({ status: "REFUSED", code: "OVERWRITE_NOT_CONFIRMED", detail: "Overwrite needs the files typed: a.txt", conflict: conflict() }), "apply", {});
  assert.equal(s.kind, "conflict");
  if (s.kind === "conflict") assert.match(s.refusal, /needs the files typed/);
  assert.equal(stepOf(ack({ status: "REFUSED", conflict: null }), "apply", {}).kind, "stopped");
});

test("Cancel is first and the default, whatever order the Core listed the options in", () => {
  assert.equal(DEFAULT_OPTION, "CANCEL");
  assert.deepEqual(optionsOf(conflict()).map((o) => o.option), ["CANCEL", "MERGE_MANUALLY", "OVERWRITE"]);
  assert.deepEqual(optionsOf(conflict({ options: [opt("MERGE_MANUALLY")] })).map((o) => o.option), ["MERGE_MANUALLY"], "only what the Core offers; nothing is invented");
});

test("an overwrite needs its files typed; the guard compares the Core's list with what was typed", () => {
  const o = opt("OVERWRITE", { destructive: true, needsConfirmation: true, confirmPaths: ["src/a.ts", "b.txt"] });
  assert.deepEqual(requiredPaths(o), ["src/a.ts", "b.txt"]);
  assert.deepEqual(requiredPaths(opt("STASH", { confirmPaths: ["x"] })), [], "a path is asked for only when the Core says the option needs it");
  assert.deepEqual(missingPaths(["src/a.ts", "b.txt"], ""), ["src/a.ts", "b.txt"]);
  assert.deepEqual(missingPaths(["src/a.ts", "b.txt"], "  src/a.ts \n\nb.txt\n"), []);
  assert.deepEqual(missingPaths(["src/a.ts"], "src\\a.ts"), [], "a Windows separator is the same path");
  assert.deepEqual(missingPaths(["src/a.ts", "b.txt"], "src/a.ts"), ["b.txt"]);
  assert.deepEqual(typedPaths("a\r\nb\n\n c "), ["a", "b", "c"]);
});

test("only a non-destructive choice that acts may be remembered", () => {
  assert.equal(canRemember(opt("MERGE_MANUALLY")), true);
  assert.equal(canRemember(opt("STASH")), true);
  assert.equal(canRemember(opt("OVERWRITE", { destructive: true })), false);
  assert.equal(canRemember(opt("FULL_OVERWRITE", { destructive: true })), false);
  assert.equal(canRemember(opt("CANCEL")), false);
  assert.equal(canRemember(opt("UNDO_AND_APPLY")), false);
});

test("the result is said from the Core's lists: files applied, markers left, files restored", () => {
  assert.equal(doneWords("apply", ack({ appliedPaths: ["a", "b"] })), "Applied 2 files to your checkout.");
  assert.equal(doneWords("apply", ack({ appliedPaths: ["a"], unresolvedPaths: ["a"] })), "Applied 1 file to your checkout. 1 file has conflict markers to resolve.");
  assert.equal(doneWords("undo", ack({ status: "UNDONE", appliedPaths: ["a", "b", "c"] })), "Restored 3 files to the bytes they had before the apply.");
  assert.match(doneWords("apply", ack({ status: "NOTHING_TO_APPLY", detail: "" })), /already holds every change/);
});

test("undo and apply asks first for the undo, then offers the apply; an undo is never described as an apply", () => {
  const pending = stepOf(ack({ status: "APPROVAL_PENDING", approvalId: "x" }), "apply", { option: "UNDO_AND_APPLY" });
  assert.equal(pending.kind === "approve" ? pending.flow : "", "undo");
  const undone = stepOf(ack({ status: "UNDONE", appliedPaths: ["a"] }), "apply", { option: "UNDO_AND_APPLY" });
  assert.equal(undone.kind === "done" ? undone.flow : "", "undo");
  assert.equal(undone.kind === "done" ? undone.reapply : null, true);
  const plain = stepOf(ack({ status: "UNDONE" }), "undo", {});
  assert.equal(plain.kind === "done" ? plain.reapply : null, false);
});
