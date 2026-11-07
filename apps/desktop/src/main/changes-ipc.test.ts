import assert from "node:assert/strict";
import { test } from "node:test";
import { create } from "@bufbuild/protobuf";
import { ChangedPathStateSchema, ChangeEffectViewSchema, ChangesStateSchema, CommitAndPushAckSchema, DecideHunkAckSchema } from "@modbit/surface-protocol";
import { commitPushInfo, decideInfo, registerChangesHandlers, stateInfo, validChangePath, validCommitPushInput, validDecideHunkInput, validRevertInput, validStagePaths } from "./changes-ipc.ts";
import type { Registrar } from "./conversation-ipc.ts";

const SID = "0123456789abcdef0123456789abcdef";
const TID = "fedcba9876543210fedcba9876543210";
const REV = "a".repeat(64);

/** A registrar whose Core client explodes when touched: a handler that reaches it has not finished validating. */
function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  const r: Registrar = {
    handle: (channel, fn) => void handlers.set(channel, fn),
    client: () => {
      throw new Error("CLIENT_TOUCHED");
    },
    sessionId: (v) => {
      if (v !== SID) throw new Error("BAD_ARGUMENT: session id must be 32 hex chars");
      return SID;
    },
    taskId: (v) => {
      if (v !== TID) throw new Error("BAD_ARGUMENT: task id must be 32 hex chars");
      return TID;
    },
    lease: () => Promise.resolve(),
  };
  registerChangesHandlers(r);
  const call = async (channel: string, ...args: unknown[]): Promise<string> => {
    const fn = handlers.get(channel);
    assert.ok(fn, `${channel} is registered`);
    try {
      await fn(...args);
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  };
  return { call, channels: () => [...handlers.keys()] };
}

test("PX-048/PX-104: the Changes app registers five typed channels and none takes a command string or an absolute path", () => {
  assert.deepEqual(harness().channels().sort(), ["changes:commitPush", "changes:decideHunk", "changes:revert", "changes:stage", "changes:state"]);
});

test("PX-048: a malformed argument is refused in main before the Core client is touched", async () => {
  const h = harness();
  const refused = async (channel: string, ...args: unknown[]) => assert.match(await h.call(channel, ...args), /BAD_ARGUMENT/, `${channel} ${JSON.stringify(args)}`);
  await refused("changes:state", "nope");
  await refused("changes:revert", "nope", TID, { path: "a.txt", wholeFile: true, expectedFileRevision: REV });
  await refused("changes:revert", SID, "nope", { path: "a.txt", wholeFile: true, expectedFileRevision: REV });
  // The path: relative, printable, inside the tree.
  for (const path of ["", "/etc/passwd", "C:\\x", "..", "a/../../b", "a\u0000b", "x".repeat(5000), 7]) await refused("changes:revert", SID, TID, { path, wholeFile: true, expectedFileRevision: REV });
  // The revision is a digest the Core named; a hunk is a whole index.
  await refused("changes:revert", SID, TID, { path: "a.txt", wholeFile: true, expectedFileRevision: "not a digest" });
  await refused("changes:revert", SID, TID, { path: "a.txt", wholeFile: false, hunkIndex: -1, expectedFileRevision: REV });
  await refused("changes:revert", SID, TID, { path: "a.txt", wholeFile: false, hunkIndex: 1.5, expectedFileRevision: REV });
  await refused("changes:revert", SID, TID, { path: "a.txt", expectedFileRevision: REV });
  await refused("changes:revert", SID, TID, { path: "a.txt", wholeFile: true, expectedFileRevision: REV, commandId: "short" });
  await refused("changes:stage", SID, TID, [], true);
  await refused("changes:stage", SID, TID, [{ path: "a.txt", expectedFileRevision: REV }], "yes");
  await refused("changes:stage", SID, TID, [{ path: "../x", expectedFileRevision: REV }], true);
  await refused("changes:stage", SID, TID, "a.txt", true);
  await refused("changes:commitPush", SID, TID, { message: "x".repeat(4001) });
  await refused("changes:commitPush", SID, TID, { message: "a\u0000b" });
  await refused("changes:commitPush", SID, TID, { message: "ok", remote: "--upload-pack=evil" });
  await refused("changes:commitPush", SID, TID, { message: "ok", remote: "a b" });
  await refused("changes:commitPush", SID, TID, { message: "ok", expectedHead: "not a commit" });
  await refused("changes:commitPush", SID, TID, { message: "ok", expectedStagedDigest: "nope" });
  // A valid request reaches the client (which here explodes): validation is not what stopped it.
  assert.match(await h.call("changes:revert", SID, TID, { path: "src/a.txt", wholeFile: true, expectedFileRevision: REV }), /CLIENT_TOUCHED/);
  assert.match(await h.call("changes:commitPush", SID, TID, { message: "Fix it", remote: "origin" }), /CLIENT_TOUCHED/);
});

test("PX-048: validators keep what is valid and name what is not", () => {
  assert.equal(validChangePath("src/lib.rs"), "src/lib.rs");
  assert.deepEqual(validRevertInput({ path: "a.txt", wholeFile: false, hunkIndex: 2, expectedFileRevision: REV }), { path: "a.txt", wholeFile: false, hunkIndex: 2, expectedFileRevision: REV, commandId: undefined });
  assert.equal(validRevertInput({ path: "gone.txt", wholeFile: true, expectedFileRevision: "" }).expectedFileRevision, "", "a deleted file has no revision");
  assert.deepEqual(validStagePaths([{ path: "a.txt", expectedFileRevision: REV }, { path: "b.txt" }]), [
    { path: "a.txt", expectedFileRevision: REV },
    { path: "b.txt", expectedFileRevision: "" },
  ]);
  assert.deepEqual(validCommitPushInput({ message: "Fix it" }), { message: "Fix it", remote: "", expectedHead: "", expectedStagedDigest: "" });
  assert.deepEqual(validCommitPushInput(undefined), { message: "", remote: "", expectedHead: "", expectedStagedDigest: "" }, "a push alone carries no message");
});

test("PX-048: the Core's answers become plain shapes: counts as decimal strings, nothing dropped", () => {
  const s = stateInfo(
    create(ChangesStateSchema, {
      workspaceRevision: 12n,
      branch: "main",
      head: "abc",
      remote: "origin",
      remoteUrl: "https://<redacted>@example.com/o/r.git",
      paths: [create(ChangedPathStateSchema, { path: "a.txt", status: "M", staged: "M", unstaged: "", fileRevision: REV, unmerged: false, protectedPath: false })],
      unpushed: 2,
      stagedDigest: REV,
      effects: [create(ChangeEffectViewSchema, { toolCallId: "t", state: "AUTHORIZED_IN_DOUBT", authorizedReceipt: "r1", resultReceipt: "", paths: ["a.txt"] })],
    }),
  );
  assert.equal(s.workspaceRevision, "12");
  assert.equal(s.paths[0]!.staged, "M");
  assert.equal(s.effects[0]!.state, "AUTHORIZED_IN_DOUBT");
  assert.equal(s.effects[0]!.resultReceipt, "");
  assert.equal(s.remoteUrl.includes("<redacted>"), true);
  const a = commitPushInfo(create(CommitAndPushAckSchema, { status: "APPROVAL_PENDING", approvalId: "x", intentHash: REV, paths: ["a.txt"], pushOnly: true, effectReceiptIds: ["e1"] }));
  assert.deepEqual([a.status, a.pushOnly, a.paths, a.effectReceiptIds], ["APPROVAL_PENDING", true, ["a.txt"], ["e1"]]);
});

test("PX-104: a per-hunk decision is validated in main and decided by the Core", async () => {
  const h = harness();
  const DIGEST = "b".repeat(64);
  const refused = async (...args: unknown[]) => assert.match(await h.call("changes:decideHunk", ...args), /BAD_ARGUMENT/, JSON.stringify(args));
  const ok = { path: "a.txt", hunkIndex: 1, decision: "REJECT", expectedFileRevision: REV, expectedHunkDigest: DIGEST, reason: "not asked for" };
  await refused("nope", TID, ok);
  await refused(SID, "nope", ok);
  for (const decision of ["", "APPROVE", "reject", 7, undefined]) await refused(SID, TID, { ...ok, decision });
  // A decision names the revision it was made on: none, or a made-up one, is refused before the Core is asked.
  await refused(SID, TID, { ...ok, expectedFileRevision: "" });
  await refused(SID, TID, { ...ok, expectedFileRevision: "latest" });
  await refused(SID, TID, { ...ok, expectedHunkDigest: "nope" });
  for (const hunkIndex of [-1, 1.5, "1", 200_000]) await refused(SID, TID, { ...ok, hunkIndex });
  for (const path of ["", "/etc/passwd", "..", "a/../b", "a\u0000b"]) await refused(SID, TID, { ...ok, path });
  await refused(SID, TID, { ...ok, reason: "x".repeat(2001) });
  await refused(SID, TID, { ...ok, reason: "a\u0000b" });
  await refused(SID, TID, { ...ok, commandId: "short" });
  assert.match(await h.call("changes:decideHunk", SID, TID, ok), /CLIENT_TOUCHED/, "a valid decision reaches the client");
  assert.deepEqual(validDecideHunkInput({ ...ok, decision: "ACCEPT", reason: undefined }), { path: "a.txt", hunkIndex: 1, decision: "ACCEPT", expectedFileRevision: REV, expectedHunkDigest: DIGEST, reason: "", commandId: undefined });
  assert.equal(validDecideHunkInput({ ...ok, expectedHunkDigest: undefined }).expectedHunkDigest, "");
});

test("PX-104: the Core's decision ack becomes a plain shape and nothing is added by main", () => {
  const a = decideInfo(create(DecideHunkAckSchema, { result: "REJECTED", path: "a.txt", hunkIndex: 2, hunkDigest: "d", decisionId: "x", attributedCallId: "c1", attributedTool: "shell.exec", runLive: true, agentNotice: "told", fileRevision: REV, beforeHash: "b", checkpointId: "k", workspaceRevision: 9n, offset: 77n, replayed: false }));
  assert.deepEqual(a, { result: "REJECTED", path: "a.txt", hunkIndex: 2, hunkDigest: "d", decisionId: "x", attributedCallId: "c1", attributedTool: "shell.exec", runLive: true, agentNotice: "told", fileRevision: REV, beforeHash: "b", checkpointId: "k", workspaceRevision: "9", offset: "77", replayed: false });
});
