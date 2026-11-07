/**
 * Main's side of the Changes app's actions (REQ-PX-048): typed,
 * argument-validated handlers over the Core's `GetChangesState`,
 * `RevertChanges`, `StageChanges` and `CommitAndPush`. Every handler validates
 * what the renderer sent before it reaches the client (REQ-EV-0103), names the
 * session on a mutation, and converts the Core's answer to the plain shapes of
 * shared/changes-types.ts. Nothing is decided and nothing is kept here: a
 * revert is the Core's ChangeTransaction, the index and the commit are the
 * Core's hardened Git runner, and the push is the Core's protected effect.
 * Main writes no file and runs no Git.
 */
import { commitAndPush, decideHunk, getChangesState, revertChanges, stageChanges } from "@modbit/ide-adapter-core";
import type { ChangesState, CommitAndPushAck, DecideHunkAck, RevertChangesAck, StageChangesAck } from "@modbit/surface-protocol";
import type { ChangeEffectInfo, ChangesStateInfo, CommitPushAckInfo, CommitPushInput, DecideHunkAckInfo, RevertAckInfo, RevertInput, StageAckInfo } from "../shared/changes-types.ts";
import { bad, validBool, validCommandId } from "./control-args.ts";
import type { Registrar } from "./conversation-ipc.ts";

const HEX64 = /^[0-9a-f]{64}$/;
const HEX_HEAD = /^[0-9a-f]{4,64}$/;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;
const MAX_MESSAGE = 4000;
const MAX_PATHS = 2000;

// --------------------------------------------------------------- validation

function validObject(v: unknown, what: string): Record<string, unknown> {
  if (v === undefined || v === null) return {};
  if (typeof v !== "object" || Array.isArray(v)) return bad(`${what} must be an object`);
  return v as Record<string, unknown>;
}

/** A workspace-relative path the person picked from the list: bounded and printable, never absolute and never leaving the tree. The Core resolves it again after symlinks. */
export function validChangePath(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 4096 || CONTROL.test(v)) return bad("path must be a printable workspace path");
  if (v.startsWith("/") || v.startsWith("\\") || /^[A-Za-z]:/.test(v) || v.split(/[\\/]/).includes("..")) return bad("path must be relative to the workspace");
  return v;
}

/** The revision of a file as the Core named it: a sha256, or "" for a file that is deleted. */
export function validFileRevision(v: unknown): string {
  if (v === undefined || v === null || v === "") return "";
  if (typeof v !== "string" || !HEX64.test(v)) return bad("a file revision is the 64-hex-digit digest the Core named");
  return v;
}

export function validRevertInput(v: unknown): { path: string; wholeFile: boolean; hunkIndex: number; expectedFileRevision: string; commandId: Uint8Array | undefined } {
  const o = validObject(v, "revert");
  const wholeFile = validBool(o.wholeFile, "wholeFile");
  const hunk = o.hunkIndex;
  if (!wholeFile && (typeof hunk !== "number" || !Number.isInteger(hunk) || hunk < 0 || hunk > 100_000)) bad("hunkIndex must be the index of a hunk the Core listed");
  return {
    path: validChangePath(o.path),
    wholeFile,
    hunkIndex: typeof hunk === "number" && !wholeFile ? hunk : 0,
    expectedFileRevision: validFileRevision(o.expectedFileRevision),
    commandId: validCommandId(o.commandId),
  };
}

const MAX_REASON = 2000;

/** A per-hunk decision (PX-103): the hunk is named by path and index, the file revision and hunk digest are the ones the Core showed, the reason is bounded text. The Core decides everything else. */
export function validDecideHunkInput(v: unknown): { path: string; hunkIndex: number; decision: "ACCEPT" | "REJECT" | "AUTO_ACCEPT"; expectedFileRevision: string; expectedHunkDigest: string; reason: string; commandId: Uint8Array | undefined } {
  const o = validObject(v, "decision");
  const hunk = o.hunkIndex;
  if (typeof hunk !== "number" || !Number.isInteger(hunk) || hunk < 0 || hunk > 100_000) bad("hunkIndex must be the index of a hunk the Core listed");
  const decision = o.decision;
  if (decision !== "ACCEPT" && decision !== "REJECT" && decision !== "AUTO_ACCEPT") bad("decision must be ACCEPT, REJECT or AUTO_ACCEPT");
  const rev = validFileRevision(o.expectedFileRevision);
  if (rev === "") bad("a decision names the file revision it was made on");
  const digest = o.expectedHunkDigest === undefined || o.expectedHunkDigest === null || o.expectedHunkDigest === "" ? "" : o.expectedHunkDigest;
  if (typeof digest !== "string" || (digest !== "" && !HEX64.test(digest))) bad("expectedHunkDigest must be the digest the Core named");
  const reason = o.reason === undefined || o.reason === null ? "" : o.reason;
  if (typeof reason !== "string" || reason.length > MAX_REASON || reason.includes("\u0000")) bad(`the reason is text of at most ${MAX_REASON} characters`);
  return { path: validChangePath(o.path), hunkIndex: hunk as number, decision: decision as "ACCEPT" | "REJECT" | "AUTO_ACCEPT", expectedFileRevision: rev, expectedHunkDigest: digest as string, reason: reason as string, commandId: validCommandId(o.commandId) };
}

export function validStagePaths(v: unknown): { path: string; expectedFileRevision: string }[] {
  if (!Array.isArray(v) || v.length === 0 || v.length > MAX_PATHS) return bad(`name between 1 and ${MAX_PATHS} paths`);
  return v.map((p) => {
    const o = validObject(p, "path");
    return { path: validChangePath(o.path), expectedFileRevision: validFileRevision(o.expectedFileRevision) };
  });
}

export function validCommitPushInput(v: unknown): { message: string; remote: string; expectedHead: string; expectedStagedDigest: string } {
  const o = validObject(v, "commit");
  const message = o.message === undefined || o.message === null ? "" : o.message;
  if (typeof message !== "string" || message.length > MAX_MESSAGE || message.includes("\u0000")) return bad(`the commit message is text of at most ${MAX_MESSAGE} characters`);
  const remote = o.remote === undefined || o.remote === null ? "" : o.remote;
  if (typeof remote !== "string" || remote.length > 128 || (remote !== "" && !/^[A-Za-z0-9][A-Za-z0-9._/-]*$/.test(remote))) return bad("remote must be the name of a configured remote");
  const head = o.expectedHead === undefined || o.expectedHead === null ? "" : o.expectedHead;
  if (typeof head !== "string" || (head !== "" && !HEX_HEAD.test(head))) return bad("expectedHead must be the commit id the app showed");
  const digest = o.expectedStagedDigest === undefined || o.expectedStagedDigest === null ? "" : o.expectedStagedDigest;
  if (typeof digest !== "string" || (digest !== "" && !HEX64.test(digest))) return bad("expectedStagedDigest must be the digest the app showed");
  return { message, remote, expectedHead: head, expectedStagedDigest: digest };
}

// --------------------------------------------------------------- conversion

export function stateInfo(s: ChangesState): ChangesStateInfo {
  return {
    workspaceRevision: s.workspaceRevision.toString(),
    branch: s.branch,
    head: s.head,
    remote: s.remote,
    remoteUrl: s.remoteUrl,
    paths: s.paths.map((p) => ({ path: p.path, status: p.status, staged: p.staged, unstaged: p.unstaged, fileRevision: p.fileRevision, unmerged: p.unmerged, protectedPath: p.protectedPath })),
    unpushed: s.unpushed,
    taskRunning: s.taskRunning,
    mergeInProgress: s.mergeInProgress,
    stagedDigest: s.stagedDigest,
    effects: s.effects.map(
      (e): ChangeEffectInfo => ({
        toolCallId: e.toolCallId,
        state: e.state,
        approvalId: e.approvalId,
        intentHash: e.intentHash,
        message: e.message,
        paths: [...e.paths],
        branch: e.branch,
        remote: e.remote,
        pushOnly: e.pushOnly,
        headBefore: e.headBefore,
        commit: e.commit,
        authorizedReceipt: e.authorizedReceipt,
        resultReceipt: e.resultReceipt,
        resultStatus: e.resultStatus,
        code: e.code,
        detail: e.detail,
      }),
    ),
    identity: s.identity,
  };
}

export function revertInfo(a: RevertChangesAck): RevertAckInfo {
  return {
    path: a.path,
    action: a.action,
    hunkIndex: a.hunkIndex,
    workspaceRevision: a.workspaceRevision.toString(),
    previousRevision: a.previousRevision.toString(),
    fileRevision: a.fileRevision,
    beforeHash: a.beforeHash,
    checkpointId: a.checkpointId,
    offset: a.offset.toString(),
    replayed: a.replayed,
  };
}

export function decideInfo(a: DecideHunkAck): DecideHunkAckInfo {
  return {
    result: a.result,
    path: a.path,
    hunkIndex: a.hunkIndex,
    hunkDigest: a.hunkDigest,
    decisionId: a.decisionId,
    attributedCallId: a.attributedCallId,
    attributedTool: a.attributedTool,
    runLive: a.runLive,
    agentNotice: a.agentNotice,
    fileRevision: a.fileRevision,
    beforeHash: a.beforeHash,
    checkpointId: a.checkpointId,
    workspaceRevision: a.workspaceRevision.toString(),
    offset: a.offset.toString(),
    replayed: a.replayed,
  };
}

export function stageInfo(a: StageChangesAck): StageAckInfo {
  return { paths: [...a.paths], staged: a.staged, stagedDigest: a.stagedDigest, offset: a.offset.toString() };
}

export function commitPushInfo(a: CommitAndPushAck): CommitPushAckInfo {
  return {
    status: a.status,
    approvalId: a.approvalId,
    intentHash: a.intentHash,
    toolCallId: a.toolCallId,
    message: a.message,
    paths: [...a.paths],
    branch: a.branch,
    remote: a.remote,
    remoteUrl: a.remoteUrl,
    headBefore: a.headBefore,
    commit: a.commit,
    pushed: a.pushed,
    pushOnly: a.pushOnly,
    effectReceiptIds: [...a.effectReceiptIds],
    replayed: a.replayed,
    code: a.code,
    detail: a.detail,
    stagedDigest: a.stagedDigest,
  };
}

// ------------------------------------------------------------- the handlers

/** The commands of the Changes app. */
export function registerChangesHandlers(r: Registrar): void {
  r.handle("changes:state", async (...a) => {
    const tid = r.taskId(a[0]);
    return stateInfo(await getChangesState(r.client(), tid));
  });
  r.handle("changes:revert", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const input = validRevertInput(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    return revertInfo(await revertChanges(c, sid, tid, { path: input.path, wholeFile: input.wholeFile, hunkIndex: input.hunkIndex, expectedFileRevision: input.expectedFileRevision }, input.commandId));
  });
  r.handle("changes:decideHunk", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const input = validDecideHunkInput(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    const { commandId, ...d } = input;
    return decideInfo(await decideHunk(c, sid, tid, d, commandId));
  });
  r.handle("changes:stage", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const paths = validStagePaths(a[2]);
    const stage = validBool(a[3], "stage");
    const c = r.client();
    await r.lease(c, sid);
    return stageInfo(await stageChanges(c, sid, tid, paths, stage));
  });
  r.handle("changes:commitPush", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const input: CommitPushInput = validCommitPushInput(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    return commitPushInfo(await commitAndPush(c, sid, tid, input));
  });
}
