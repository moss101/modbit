import { test } from "node:test";
import assert from "node:assert/strict";
import type { CheckpointInfo, CheckpointListInfo, RewindEntryInfo, RewindPreviewInfo } from "../../shared/control-types.ts";
import { allChosen, byTurn, undoAllTarget, changeCounts, checkpointBefore, editedAndAffected, expectedHashes, keepPathsOf, newCommandId, normTurn, redoCandidate, refusalWords, restoreSentence } from "./checkpoints-model.ts";

const T1 = "11111111111111111111111111111111";
const T2 = "22222222222222222222222222222222";
const cp = (over: Partial<CheckpointInfo>): CheckpointInfo => ({ checkpointId: "c", epoch: 1, kind: "DELTA", status: "SUPERSEDED", reason: "", files: 0, removed: 0, eventOffset: "0", createdAtMs: 0, name: "", turnId: "", turnOrdinal: 0, retention: [], ...over });
const list = (checkpoints: CheckpointInfo[]): CheckpointListInfo => ({ checkpoints, currentCheckpointId: checkpoints.at(-1)?.checkpointId ?? "", currentEpoch: checkpoints.at(-1)?.epoch ?? 0 });

const L = list([cp({ checkpointId: "a", epoch: 1, turnId: T1, turnOrdinal: 1, eventOffset: "10" }), cp({ checkpointId: "b", epoch: 2, turnId: T2, turnOrdinal: 2, eventOffset: "20" }), cp({ checkpointId: "x", epoch: 3, reason: "requested", eventOffset: "25" })]);

test("REQ-PX-062: a turn's checkpoint is found by its turn id, hyphenated or not; a checkpoint that is not a turn's is not a marker", () => {
  const m = byTurn(L);
  assert.equal(m.get(normTurn("11111111-1111-1111-1111-111111111111"))?.checkpointId, "a");
  assert.equal(m.get(T2)?.checkpointId, "b");
  assert.equal(m.size, 2);
  assert.equal(byTurn(null).size, 0);
  assert.equal(byTurn(list([cp({ turnId: T1, status: "REJECTED" })])).size, 0, "an uncommitted checkpoint is no marker");
});

test("REQ-PX-062: the checkpoint for a message is the newest turn boundary recorded before it was sent; the first message has none", () => {
  assert.equal(checkpointBefore(L, "5"), null);
  assert.equal(checkpointBefore(L, "15")?.checkpointId, "a");
  assert.equal(checkpointBefore(L, "21")?.checkpointId, "b");
  assert.equal(checkpointBefore(L, "9999")?.checkpointId, "b", "a checkpoint that is not a turn's is never the answer");
  assert.equal(checkpointBefore(null, "5"), null);
  assert.equal(checkpointBefore(L, "not a number"), null);
});

test("REQ-PX-062: Redo is offered for the newest pre-restore checkpoint until work moves on or the person redid it", () => {
  const pre = cp({ checkpointId: "pre", epoch: 4, reason: "pre_restore", retention: ["PRE_RESTORE", "LATEST"] });
  const withPre = list([...L.checkpoints, pre]);
  assert.equal(redoCandidate(withPre, new Set())?.checkpointId, "pre");
  assert.equal(redoCandidate(withPre, new Set(["pre"])), null);
  const moved = list([...withPre.checkpoints, cp({ checkpointId: "t3", epoch: 5, turnId: "33333333333333333333333333333333", turnOrdinal: 3 })]);
  assert.equal(redoCandidate(moved, new Set()), null, "a turn recorded after the restore makes the redo inexact");
  assert.equal(redoCandidate(L, new Set()), null);
});

test("REQ-PX-062: Undo all restores the first turn's checkpoint only when it recorded no changed file", () => {
  const first = cp({ checkpointId: "first", epoch: 1, turnId: T1, turnOrdinal: 1, files: 0 });
  assert.equal(undoAllTarget(list([first, cp({ checkpointId: "b", epoch: 2, turnId: T2, turnOrdinal: 2, files: 2 })]))?.checkpointId, "first");
  assert.equal(undoAllTarget(list([{ ...first, files: 1 }, cp({ checkpointId: "b", epoch: 2, turnId: T2, turnOrdinal: 2 })])), null, "the first turn already changed a file: nothing recorded before it");
  assert.equal(undoAllTarget(list([cp({ checkpointId: "x", epoch: 1, reason: "requested" })])), null);
  assert.equal(undoAllTarget(null), null);
});

const e = (path: string, action: string): RewindEntryInfo => ({ path, action, currentHash: `cur-${path}`, targetHash: "" });
const preview = (entries: RewindEntryInfo[], userEdited: string[] = []): RewindPreviewInfo => ({ refusal: "", detail: "", checkpointId: "a", epoch: 1, filesWritten: 0, filesReverted: 0, entries, userEdited, currentEpoch: 3 });

test("REQ-PX-062: the dialog counts what the Core's preview will do, in words", () => {
  const c = changeCounts([e("a", "WRITE"), e("b", "DELETE"), e("c", "REMOVE_UNTRACKED"), e("d", "REVERT_TO_HEAD"), e("f", "UNCHANGED")]);
  assert.deepEqual(c, { written: 1, deleted: 2, reverted: 1, unchanged: 1 });
  assert.match(restoreSentence(c), /1 file is put back to the checkpoint.*1 file is put back to the last commit.*2 files are deleted/);
  assert.match(restoreSentence(changeCounts([e("f", "UNCHANGED")])), /Nothing in the workspace differs/);
});

test("REQ-PX-062: files the person edited need a choice only if the restore would change them; Keep leaves them; hashes sent are the ones shown", () => {
  const p = preview([e("a", "WRITE"), e("b", "WRITE"), e("c", "UNCHANGED")], ["b", "c", "z"]);
  assert.deepEqual(editedAndAffected(p), ["b"]);
  assert.deepEqual(expectedHashes(p), [{ path: "a", contentHash: "cur-a" }, { path: "b", contentHash: "cur-b" }]);
  assert.equal(allChosen(["b"], {}), false);
  assert.equal(allChosen(["b"], { b: "keep" }), true);
  assert.deepEqual(keepPathsOf({ b: "keep", c: "overwrite", d: undefined }), ["b"]);
  assert.equal(allChosen([], {}), true);
});

test("REQ-PX-062: typed refusals are explained and a command id is 32 hex characters", () => {
  assert.match(refusalWords("REDO_SUPERSEDED", ""), /no longer give back exactly/);
  assert.match(refusalWords("HASH_MISMATCH", "x changed"), /changed after this dialog was opened/);
  assert.match(refusalWords("OBJECT_MISMATCH", "d"), /object mismatch/i);
  assert.match(newCommandId(), /^[0-9a-f]{32}$/);
});
