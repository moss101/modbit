/**
 * Typed commands of the Changes app (REQ-PX-048): the state beside the review
 * bundle, per-file and per-hunk revert, stage and unstage, and Commit and
 * push. Thin calls over the one SurfaceProtocol client: the Core decides
 * (staleness, protected paths, dirty conflicts, the approval and the push) and
 * these return its answer unchanged, or throw its typed refusal. Every mutation
 * names the session and takes the lease the client holds.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ChangesStateSchema,
  CommitAndPushAckSchema,
  CommitAndPushSchema,
  GetChangesStateSchema,
  RevertChangesAckSchema,
  RevertChangesSchema,
  StageChangesAckSchema,
  StageChangesSchema,
  type ChangesState,
  type CommitAndPushAck,
  type RevertChangesAck,
  type StageChangesAck,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });
const lease = (c: CoreClient, sessionId: string): bigint | undefined => c.leaseGeneration(sessionId);

export async function getChangesState(c: CoreClient, taskId: string): Promise<ChangesState> {
  const payload = toBinary(GetChangesStateSchema, create(GetChangesStateSchema, { taskId: id(taskId) }));
  return fromBinary(ChangesStateSchema, (await c.command("GetChangesState", payload)).result);
}

export interface RevertTarget {
  path: string;
  /** Revert the whole file to HEAD; otherwise one hunk. */
  wholeFile: boolean;
  hunkIndex?: number;
  /** The revision of the file the client saw ("" for a file that is deleted). */
  expectedFileRevision: string;
}

export async function revertChanges(c: CoreClient, sessionId: string, taskId: string, t: RevertTarget, commandId?: Uint8Array): Promise<RevertChangesAck> {
  const payload = toBinary(RevertChangesSchema, create(RevertChangesSchema, { taskId: id(taskId), path: t.path, wholeFile: t.wholeFile, hunkIndex: t.hunkIndex ?? 0, expectedFileRevision: t.expectedFileRevision }));
  return fromBinary(RevertChangesAckSchema, (await c.command("RevertChanges", payload, commandId, lease(c, sessionId), sessionId)).result);
}

export async function stageChanges(c: CoreClient, sessionId: string, taskId: string, paths: { path: string; expectedFileRevision: string }[], stage: boolean): Promise<StageChangesAck> {
  const payload = toBinary(StageChangesSchema, create(StageChangesSchema, { taskId: id(taskId), paths, stage }));
  return fromBinary(StageChangesAckSchema, (await c.command("StageChanges", payload, undefined, lease(c, sessionId), sessionId)).result);
}

export interface CommitAndPushInput {
  message: string;
  remote?: string;
  expectedHead?: string;
  expectedStagedDigest?: string;
}

/** The first call answers APPROVAL_PENDING with the exact intent; after the approval is resolved the same call runs it. */
export async function commitAndPush(c: CoreClient, sessionId: string, taskId: string, a: CommitAndPushInput): Promise<CommitAndPushAck> {
  const payload = toBinary(CommitAndPushSchema, create(CommitAndPushSchema, { taskId: id(taskId), message: a.message, remote: a.remote ?? "", expectedHead: a.expectedHead ?? "", expectedStagedDigest: a.expectedStagedDigest ?? "" }));
  return fromBinary(CommitAndPushAckSchema, (await c.command("CommitAndPush", payload, undefined, lease(c, sessionId), sessionId)).result);
}
