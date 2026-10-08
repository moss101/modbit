/**
 * Main's side of the approval stack, the run modes, the context ring and the
 * checkpoint surface (REQ-PX-057, 058, 060, 062): typed, argument-validated
 * handlers over the Core's own commands. Nothing is decided here. The Core
 * says which mode approves what, whether a rule is valid, what a restore
 * would change and whether a redo is still exact; this file validates what
 * the renderer sent, calls the one client and converts the Core's answer to
 * plain shapes (64-bit counts as decimal strings). docs/81: the renderer holds
 * no authority, and main holds no policy.
 */
import type { CoreClient } from "@modbit/ide-adapter-core";
import { addAllowRule, forkTask, getContextAccounting, getRunMode, listAllowRules, listCheckpoints, previewRewind, readObjectRange, restoreCheckpoint, revokeAllowRule, setRunMode, setTaskBudgets } from "@modbit/ide-adapter-core";
import { TranscriptDensity, type AllowRuleView, type ContextAccountingView, type RunModeView, type TranscriptRow } from "@modbit/surface-protocol";
import type { AccountingInfo, AllowRuleInfo, CheckpointListInfo, DockApprovalView, ForkOutcome, RestoreOutcome, RewindPreviewInfo, RunModeInfo, TaskBudgetsInfo } from "../shared/control-types.ts";
import { intentOf, reasonOf } from "./approval-intent.ts";
import { bad, validApprovalId, validBool, validCommandId, validCount, validExpected, validGoal, validIntentHash, validKeepPaths, validReason, validRuleId, validRuleInput, validRunMode, validTarget } from "./control-args.ts";
import type { Registrar } from "./conversation-ipc.ts";

const hexOf = (b: Uint8Array | undefined): string => Buffer.from(b ?? []).toString("hex");
const msOf = (n: bigint | number): number => Number(n);
const norm = (id: string): string => id.replace(/-/g, "").toLowerCase();

export function runModeInfo(v: RunModeView): RunModeInfo {
  return { taskId: hexOf(v.taskId?.value), mode: v.mode, acknowledged: v.acknowledged, alwaysAsk: v.alwaysAsk, modes: v.modes, warning: v.warning, sessionOnly: v.sessionOnly, offset: v.offset.toString(), rulesInForce: v.rulesInForce };
}

export function ruleInfo(r: AllowRuleView): AllowRuleInfo {
  return { ruleId: r.ruleId, pattern: r.pattern, scope: r.scope, scopeKey: r.scopeKey, createdBy: r.createdBy, createdAtMs: msOf(r.createdAtMs), expiresAtMs: msOf(r.expiresAtMs), coversAlwaysAsk: r.coversAlwaysAsk, state: r.state, revokedBy: r.revokedBy, originTask: r.originTask, offset: r.offset.toString() };
}

export function accountingInfo(v: ContextAccountingView): AccountingInfo {
  const b = v.budgets;
  return {
    available: v.available,
    totalTokens: v.totalTokens.toString(),
    totalSource: v.totalSource,
    windowTokens: v.windowTokens.toString(),
    windowSource: v.windowSource,
    usedBp: v.usedBp,
    estimatorErrorBp: v.estimatorErrorBp,
    declaredErrorBp: v.declaredErrorBp,
    model: v.model,
    turnOrdinal: v.turnOrdinal,
    categories: v.categories.map((c) => ({ category: c.category, tokens: c.tokens.toString(), estimatedTokens: c.estimatedTokens.toString(), shareBp: c.shareBp, sources: c.sources })),
    compactionEpoch: v.compactionEpoch,
    compactionSummaries: v.compactionSummaries,
    budgets: b
      ? { maxCostMinor: b.maxCostMinor.toString(), spentMinor: b.spentMinor.toString(), heldByChildrenMinor: b.heldByChildrenMinor.toString(), maxWallMs: b.maxWallMs.toString(), wallMsUsed: b.wallMsUsed.toString(), maxChildren: b.maxChildren, liveChildren: b.liveChildren, forbidSpawn: b.forbidSpawn }
      : null,
  };
}

// ------------------------------------------------- the exact intent of a call

/**
 * The recorded arguments of a tool call, read from the Core: the call's row in
 * the transcript names the content-addressed object, and the object is read
 * in one bounded range. Cached per call (a call's arguments never change).
 */
const argsCache = new Map<string, string | null>();
const MAX_CACHE = 400;
const MAX_ARG_BYTES = 64 * 1024;

function findTool(rows: readonly TranscriptRow[], toolCallId: string): string | null {
  for (const r of rows) {
    if (r.facts.case === "tool" && norm(r.facts.value.toolCallId) === toolCallId && r.facts.value.argumentsRef) return r.facts.value.argumentsRef;
    const inner = findTool(r.children, toolCallId);
    if (inner) return inner;
  }
  return null;
}

async function argumentsOf(c: CoreClient, taskId: string, toolCallId: string): Promise<string | null> {
  const key = `${taskId}:${toolCallId}`;
  if (argsCache.has(key)) return argsCache.get(key) ?? null;
  let text: string | null = null;
  try {
    // The call is among the newest rows of its task: read the tail of the flat (DETAILED) view.
    const head = await c.getTranscript(taskId, { density: TranscriptDensity.DETAILED, afterRow: 0, limit: 1 });
    const total = head.totalRows;
    const tail = await c.getTranscript(taskId, { density: TranscriptDensity.DETAILED, afterRow: Math.max(0, total - 120), limit: 200 });
    const ref = findTool(tail.rows, toolCallId) ?? findTool(head.rows, toolCallId);
    if (ref && /^[0-9a-f]{64}$/.test(ref)) {
      const chunk = await readObjectRange(c, ref, 0n, BigInt(MAX_ARG_BYTES));
      text = Buffer.from(chunk.data).toString("utf8");
    }
  } catch {
    text = null;
  }
  if (argsCache.size >= MAX_CACHE) argsCache.delete(argsCache.keys().next().value as string);
  argsCache.set(key, text);
  return text;
}

/** Every protected effect of the session still waiting for a decision, oldest first, with the typed reason and the exact intent. */
export async function dockApprovals(c: CoreClient, sessionId: string, taskId: string | null): Promise<DockApprovalView[]> {
  const list = await c.listApprovals(sessionId);
  const pending = list.approvals.filter((a) => a.status === "REQUESTED" && (taskId === null || hexOf(a.taskId?.value) === taskId));
  pending.sort((a, b) => (a.requestedAtMs < b.requestedAtMs ? -1 : a.requestedAtMs > b.requestedAtMs ? 1 : 0));
  const out: DockApprovalView[] = [];
  for (const a of pending) {
    const tid = hexOf(a.taskId?.value);
    const callId = hexOf(a.toolCallId?.value);
    out.push({
      approvalId: hexOf(a.approvalId?.value),
      taskId: tid,
      toolCallId: callId,
      toolName: a.toolName,
      effectClass: a.effectClass,
      intentHash: a.intentHash,
      requestedAtMs: msOf(a.requestedAtMs),
      expiresAtMs: msOf(a.expiresAtMs),
      reason: reasonOf(a.scopeJson),
      intent: intentOf(await argumentsOf(c, tid, callId)),
    });
  }
  return out;
}

// ------------------------------------------------------------ the handlers

export function registerControlHandlers(r: Registrar): void {
  r.handle("approvals:dock", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = a[1] === undefined || a[1] === null || a[1] === "" ? null : r.taskId(a[1]);
    return dockApprovals(r.client(), sid, tid);
  });

  /**
   * "Always allow this prefix": the rule is made first, and only when the Core
   * accepts it is this one effect approved. The approval is named by the hash
   * the card showed and is checked against the Core's open approval here as well
   * as by the Core on the decision itself: a card whose effect changed under it
   * (a retry with other parameters is a new approval with another hash) is refused.
   */
  r.handle("approval:allowAlways", async (...a) => {
    const sid = r.sessionId(a[0]);
    const approvalId = validApprovalId(a[1]);
    const hash = validIntentHash(a[2]);
    const rule = validRuleInput(a[3], Date.now());
    const c = r.client();
    await r.lease(c, sid);
    const open = (await c.listApprovals(sid)).approvals.find((x) => hexOf(x.approvalId?.value) === approvalId);
    if (!open || open.status !== "REQUESTED") bad("STALE_APPROVAL: the effect is no longer waiting for a decision");
    if (open!.intentHash !== hash) bad("INTENT_MISMATCH: the effect on this card is not the one the Core is asking about");
    const taskId = hexOf(open!.taskId?.value);
    const created = await addAllowRule(c, sid, taskId, rule);
    try {
      const res = await c.resolveApproval(sid, approvalId, true, `approved, and always allowed by rule ${created.ruleId}`, hash);
      return { rule: ruleInfo(created), approvalId, status: res.status, offset: res.offset.toString() };
    } catch (e) {
      // The effect was not approved, so the rule made for it is taken back: a refused card leaves no standing permission behind.
      await revokeAllowRule(c, sid, taskId, created.ruleId, "the approval it was made with was refused").catch(() => undefined);
      throw e;
    }
  });

  r.handle("runmode:get", async (...a) => {
    const tid = r.taskId(a[0]);
    return runModeInfo(await getRunMode(r.client(), tid));
  });
  r.handle("runmode:set", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const mode = validRunMode(a[2]);
    const ack = validBool(a[3], "acknowledgeRisk");
    const c = r.client();
    await r.lease(c, sid);
    return runModeInfo(await setRunMode(c, sid, tid, mode, ack));
  });
  r.handle("rules:list", async (...a) => {
    const tid = r.taskId(a[0]);
    const inactive = a[1] === undefined ? false : validBool(a[1], "includeInactive");
    const c = r.client();
    return (await listAllowRules(c, tid, inactive)).rules.map(ruleInfo);
  });
  r.handle("rules:add", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const rule = validRuleInput(a[2], Date.now());
    const c = r.client();
    await r.lease(c, sid);
    return ruleInfo(await addAllowRule(c, sid, tid, rule));
  });
  r.handle("rules:revoke", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const ruleId = validRuleId(a[2]);
    const reason = validReason(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    const res = await revokeAllowRule(c, sid, tid, ruleId, reason);
    return { ruleId: res.ruleId, offset: res.offset.toString() };
  });

  r.handle("accounting:get", async (...a) => {
    const tid = r.taskId(a[0]);
    return accountingInfo(await getContextAccounting(r.client(), tid));
  });
  r.handle("budgets:set", async (...a): Promise<TaskBudgetsInfo> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const o = (a[2] ?? {}) as { maxCostMinor?: unknown; maxWallMs?: unknown };
    if (typeof o !== "object" || Array.isArray(o)) bad("budgets must be an object");
    const maxCostMinor = validCount(o.maxCostMinor, "maxCostMinor");
    const maxWallMs = validCount(o.maxWallMs, "maxWallMs");
    const c = r.client();
    await r.lease(c, sid);
    const v = await setTaskBudgets(c, sid, tid, { maxCostMinor, maxWallMs, maxChildren: 0, forbidSpawn: false });
    return { maxCostMinor: v.maxCostMinor.toString(), maxWallMs: v.maxWallMs.toString(), maxChildren: v.maxChildren, forbidSpawn: v.forbidSpawn, offset: v.offset.toString() };
  });

  r.handle("checkpoints:list", async (...a) => {
    const tid = r.taskId(a[0]);
    return checkpointList(await listCheckpoints(r.client(), tid));
  });
  r.handle("checkpoints:preview", async (...a): Promise<RewindPreviewInfo> => {
    const tid = r.taskId(a[0]);
    const target = validTarget(a[1]);
    const c = r.client();
    const [pv, list] = await Promise.all([previewRewind(c, tid, target), listCheckpoints(c, tid)]);
    // What the person changed outside the agent's turns: how the working copy differs from the task's latest checkpoint
    // (after a restore the Core records the restored state as the latest, so this holds there too).
    let userEdited: string[] = [];
    if (!pv.refusal) {
      const now = await previewRewind(c, tid, {}).catch(() => null);
      if (now && !now.refusal) userEdited = now.entries.filter((e) => e.action !== "UNCHANGED").map((e) => e.path);
    }
    return {
      refusal: pv.refusal,
      detail: pv.detail,
      checkpointId: pv.checkpointId,
      epoch: pv.epoch,
      filesWritten: pv.filesWritten,
      filesReverted: pv.filesReverted,
      entries: pv.entries.map((e) => ({ path: e.path, action: e.action, currentHash: e.currentHash, targetHash: e.targetHash })),
      userEdited,
      currentEpoch: list.currentEpoch,
    };
  });
  r.handle("checkpoints:restore", async (...a): Promise<RestoreOutcome> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const target = validTarget(a[2]);
    const o = (a[3] ?? {}) as { expected?: unknown; keepPaths?: unknown; redo?: unknown; expectedCurrentEpoch?: unknown; commandId?: unknown };
    if (typeof o !== "object" || Array.isArray(o)) bad("options must be an object");
    const expectedCurrentEpoch = o.expectedCurrentEpoch === undefined ? 0 : o.expectedCurrentEpoch;
    if (typeof expectedCurrentEpoch !== "number" || !Number.isInteger(expectedCurrentEpoch) || expectedCurrentEpoch < 0 || expectedCurrentEpoch > 1_000_000) bad("expectedCurrentEpoch must be the epoch the preview showed");
    const redo = o.redo === undefined ? false : validBool(o.redo, "redo");
    if (redo && !target.checkpointId) bad("a redo names the pre-restore checkpoint");
    const expected = validExpected(o.expected);
    const keepPaths = validKeepPaths(o.keepPaths);
    const commandId = validCommandId(o.commandId);
    const c = r.client();
    await r.lease(c, sid);
    const res = await restoreCheckpoint(c, sid, tid, target, { expected, keepPaths, redo, expectedCurrentEpoch: expectedCurrentEpoch as number }, commandId);
    return {
      restored: res.restored,
      refusal: res.refusal,
      detail: res.detail,
      checkpointId: res.checkpointId,
      epoch: res.epoch,
      filesWritten: res.filesWritten,
      filesReverted: res.filesReverted,
      preRestoreCheckpointId: res.preRestoreCheckpointId,
      keptPaths: res.keptPaths,
      redo: res.redo,
      replayed: res.replayed,
    };
  });
  r.handle("checkpoints:fork", async (...a): Promise<ForkOutcome> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const target = validTarget(a[2]);
    const goal = validGoal(a[3]);
    const commandId = validCommandId(a[4]);
    if (target.name !== undefined) bad("a fork names a checkpoint, a turn or a turn number");
    const c = r.client();
    await r.lease(c, sid);
    const f = await forkTask(c, sid, tid, { ...(target.checkpointId ? { checkpointId: target.checkpointId } : {}), ...(target.turnId ? { turnId: target.turnId } : {}), ...(target.turnOrdinal ? { turnOrdinal: target.turnOrdinal } : {}), goalText: goal }, commandId);
    return { taskId: hexOf(f.taskId?.value), sourceTaskId: hexOf(f.sourceTaskId?.value), checkpointId: f.checkpointId, turnOrdinal: f.turnOrdinal, filesMaterialized: f.filesMaterialized, approvalsDropped: f.approvalsDropped, worktree: f.worktree, branch: f.branch };
  });
}

export function checkpointList(l: Awaited<ReturnType<typeof listCheckpoints>>): CheckpointListInfo {
  return {
    currentCheckpointId: l.currentCheckpointId,
    currentEpoch: l.currentEpoch,
    checkpoints: l.checkpoints.map((c) => ({
      checkpointId: c.checkpointId,
      epoch: c.epoch,
      kind: c.kind,
      status: c.status,
      reason: c.reason,
      files: c.files,
      removed: c.removed,
      eventOffset: c.eventOffset.toString(),
      createdAtMs: msOf(c.createdAtMs),
      name: c.name,
      turnId: hexOf(c.turnId?.value),
      turnOrdinal: c.turnOrdinal,
      retention: c.retention,
    })),
  };
}
