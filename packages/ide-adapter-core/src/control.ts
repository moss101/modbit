/**
 * Typed commands of the approval stack, the run modes, the context ring and
 * the checkpoint surface (REQ-PX-057, 058, 060, 062), over the one
 * SurfaceProtocol client. Every function here is a thin typed call: the Core
 * decides (which mode approves what, which rule is valid, what a restore
 * changes, whether a redo is still exact) and these return its answer
 * unchanged. A mutation takes the session lease the client holds, so a
 * client that does not hold it is refused by the Core, never by this file.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  AddAllowRuleSchema,
  AllowRuleListSchema,
  AllowRuleRevokedSchema,
  AllowRuleViewSchema,
  CheckpointListSchema,
  CheckpointRestoreResultSchema,
  ContextAccountingViewSchema,
  ForkTaskSchema,
  GetContextAccountingSchema,
  GetRunModeSchema,
  ListAllowRulesSchema,
  ListCheckpointsSchema,
  NameCheckpointSchema,
  CheckpointNamedSchema,
  ObjectRangeChunkSchema,
  PreviewRewindSchema,
  ReadObjectRangeSchema,
  RestoreCheckpointSchema,
  RevokeAllowRuleSchema,
  RewindPreviewSchema,
  RunModeViewSchema,
  SetRunModeSchema,
  SetTaskBudgetsSchema,
  TaskBudgetsViewSchema,
  TaskForkedSchema,
  type AllowRuleList,
  type AllowRuleRevoked,
  type AllowRuleView,
  type CheckpointList,
  type CheckpointNamed,
  type CheckpointRestoreResult,
  type ContextAccountingView,
  type ObjectRangeChunk,
  type RewindPreview,
  type RunModeView,
  type TaskBudgetsView,
  type TaskForked,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });

/** What names the checkpoint a preview, restore or fork is about; at most one is given (none = the current checkpoint). */
export interface CheckpointTarget {
  checkpointId?: string;
  name?: string;
  turnOrdinal?: number;
  /** A turn id as 32 hex characters. */
  turnId?: string;
}

const lease = (c: CoreClient, sessionId: string): bigint | undefined => c.leaseGeneration(sessionId);

export async function getRunMode(c: CoreClient, taskId: string): Promise<RunModeView> {
  const ack = await c.command("GetRunMode", toBinary(GetRunModeSchema, create(GetRunModeSchema, { taskId: id(taskId) })));
  return fromBinary(RunModeViewSchema, ack.result);
}

export async function setRunMode(c: CoreClient, sessionId: string, taskId: string, mode: string, acknowledgeRisk: boolean): Promise<RunModeView> {
  const payload = toBinary(SetRunModeSchema, create(SetRunModeSchema, { taskId: id(taskId), mode, acknowledgeRisk }));
  const ack = await c.command("SetRunMode", payload, undefined, lease(c, sessionId));
  return fromBinary(RunModeViewSchema, ack.result);
}

export async function addAllowRule(c: CoreClient, sessionId: string, taskId: string, rule: { pattern: string[]; scope: string; expiresAtMs: number; coversAlwaysAsk: boolean }): Promise<AllowRuleView> {
  const payload = toBinary(AddAllowRuleSchema, create(AddAllowRuleSchema, { taskId: id(taskId), pattern: rule.pattern, scope: rule.scope, expiresAtMs: BigInt(rule.expiresAtMs), coversAlwaysAsk: rule.coversAlwaysAsk }));
  const ack = await c.command("AddAllowRule", payload, undefined, lease(c, sessionId));
  return fromBinary(AllowRuleViewSchema, ack.result);
}

export async function revokeAllowRule(c: CoreClient, sessionId: string, taskId: string, ruleId: string, reason: string): Promise<AllowRuleRevoked> {
  const payload = toBinary(RevokeAllowRuleSchema, create(RevokeAllowRuleSchema, { taskId: id(taskId), ruleId, reason }));
  const ack = await c.command("RevokeAllowRule", payload, undefined, lease(c, sessionId));
  return fromBinary(AllowRuleRevokedSchema, ack.result);
}

export async function listAllowRules(c: CoreClient, taskId: string, includeInactive: boolean): Promise<AllowRuleList> {
  const ack = await c.command("ListAllowRules", toBinary(ListAllowRulesSchema, create(ListAllowRulesSchema, { taskId: id(taskId), includeInactive })));
  return fromBinary(AllowRuleListSchema, ack.result);
}

export async function getContextAccounting(c: CoreClient, taskId: string): Promise<ContextAccountingView> {
  const ack = await c.command("GetContextAccounting", toBinary(GetContextAccountingSchema, create(GetContextAccountingSchema, { taskId: id(taskId) })));
  return fromBinary(ContextAccountingViewSchema, ack.result);
}

/** REQ-PX-116: the cost, wall-clock and delegation limits of a task (0 = none for cost and wall clock). */
export async function setTaskBudgets(c: CoreClient, sessionId: string, taskId: string, b: { maxCostMinor: bigint; maxWallMs: bigint; maxChildren: number; forbidSpawn: boolean }): Promise<TaskBudgetsView> {
  const payload = toBinary(SetTaskBudgetsSchema, create(SetTaskBudgetsSchema, { taskId: id(taskId), maxCostMinor: b.maxCostMinor, maxWallMs: b.maxWallMs, maxChildren: b.maxChildren, forbidSpawn: b.forbidSpawn }));
  const ack = await c.command("SetTaskBudgets", payload, undefined, lease(c, sessionId));
  return fromBinary(TaskBudgetsViewSchema, ack.result);
}

export async function listCheckpoints(c: CoreClient, taskId: string): Promise<CheckpointList> {
  const ack = await c.command("ListCheckpoints", toBinary(ListCheckpointsSchema, create(ListCheckpointsSchema, { taskId: id(taskId) })));
  return fromBinary(CheckpointListSchema, ack.result);
}

export async function previewRewind(c: CoreClient, taskId: string, t: CheckpointTarget): Promise<RewindPreview> {
  const payload = toBinary(
    PreviewRewindSchema,
    create(PreviewRewindSchema, { taskId: id(taskId), checkpointId: t.checkpointId ?? "", name: t.name ?? "", turnOrdinal: t.turnOrdinal ?? 0, ...(t.turnId ? { turnId: id(t.turnId) } : {}) }),
  );
  const ack = await c.command("PreviewRewind", payload);
  return fromBinary(RewindPreviewSchema, ack.result);
}

/**
 * Restore a checkpoint. `expected` are the content hashes the caller last saw
 * in a preview (the Core refuses HASH_MISMATCH if a file moved since), `keepPaths` the
 * files the person chose to leave as they are, `redo` marks the target as a
 * pre-restore checkpoint. `commandId` makes a retry restore once.
 */
export async function restoreCheckpoint(
  c: CoreClient,
  sessionId: string,
  taskId: string,
  t: CheckpointTarget,
  o: { expected: { path: string; contentHash: string }[]; keepPaths: string[]; redo: boolean; expectedCurrentEpoch: number },
  commandId?: Uint8Array,
): Promise<CheckpointRestoreResult> {
  const payload = toBinary(
    RestoreCheckpointSchema,
    create(RestoreCheckpointSchema, {
      taskId: id(taskId),
      checkpointId: t.checkpointId ?? "",
      name: t.name ?? "",
      turnOrdinal: t.turnOrdinal ?? 0,
      ...(t.turnId ? { turnId: id(t.turnId) } : {}),
      expected: o.expected.map((e) => ({ path: e.path, contentHash: e.contentHash })),
      keepPaths: o.keepPaths,
      redo: o.redo,
      expectedCurrentEpoch: o.expectedCurrentEpoch,
    }),
  );
  const ack = await c.command("RestoreCheckpoint", payload, commandId, lease(c, sessionId));
  return fromBinary(CheckpointRestoreResultSchema, ack.result);
}

export async function forkTask(c: CoreClient, sessionId: string, taskId: string, t: { checkpointId?: string; turnId?: string; turnOrdinal?: number; checkpointName?: string; goalText?: string }, commandId?: Uint8Array): Promise<TaskForked> {
  const payload = toBinary(
    ForkTaskSchema,
    create(ForkTaskSchema, { taskId: id(taskId), checkpointId: t.checkpointId ?? "", goalText: t.goalText ?? "", carry: [], worktreeDir: "", turnOrdinal: t.turnOrdinal ?? 0, checkpointName: t.checkpointName ?? "", ...(t.turnId ? { turnId: id(t.turnId) } : {}) }),
  );
  const ack = await c.command("ForkTask", payload, commandId, lease(c, sessionId));
  return fromBinary(TaskForkedSchema, ack.result);
}

export async function nameCheckpoint(c: CoreClient, sessionId: string, taskId: string, checkpointId: string, name: string): Promise<CheckpointNamed> {
  const payload = toBinary(NameCheckpointSchema, create(NameCheckpointSchema, { taskId: id(taskId), checkpointId, name }));
  const ack = await c.command("NameCheckpoint", payload, undefined, lease(c, sessionId));
  return fromBinary(CheckpointNamedSchema, ack.result);
}

/** One bounded range of a content-addressed object (a tool call's arguments, say). */
export async function readObjectRange(c: CoreClient, objectHash: string, offset: bigint, length: bigint): Promise<ObjectRangeChunk> {
  const ack = await c.command("ReadObjectRange", toBinary(ReadObjectRangeSchema, create(ReadObjectRangeSchema, { objectHash, offset, length })));
  return fromBinary(ObjectRangeChunkSchema, ack.result);
}
