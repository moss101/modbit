/**
 * Main's side of the automations surface (REQ-PX-086; docs/68): typed,
 * argument-validated handlers over the Core's automation commands. Nothing is
 * decided here. The Core validates a definition, versions it, binds the enable
 * approval to its hash, runs, pauses and kills; this file validates what the
 * renderer sent, calls the one client and converts the Core's answer to plain
 * shapes. A Core refusal reaches the renderer as its own `CODE: message`
 * (APPROVAL_MISMATCH, APPROVAL_LISTS_DIFFER, REPOSITORY_UNTRUSTED,
 * INVALID_DEFINITION, NOT_ENABLED, ...). docs/81: the renderer holds no
 * authority, and main holds no policy.
 */
import type { AgentHeaderView } from "../shared/conversation-types.ts";
import type {
  AutomationAttention,
  AutomationDetail,
  AutomationIssue,
  AutomationList,
  AutomationRunStarted,
  AutomationRunView,
  AutomationTriggerView,
  AutomationValidation,
  AutomationView,
  KillReport,
  RepositoryAutomations,
} from "@modbit/surface-protocol";
import { ackAutomationAttention, createAutomation, disableAutomation, enableAutomation, getAutomation, killAutomation, listAutomationRuns, listAutomations, loadRepositoryAutomations, pauseAutomation, runAutomation, updateAutomation, validateAutomation } from "@modbit/ide-adapter-core";
import type { AutoAttention, AutoDetail, AutoIssue, AutoKillReport, AutoList, AutoRepoLoad, AutoRun, AutoRunStarted, AutoTrigger, AutoValidation, AutoView } from "../shared/automation-types.ts";
import { validAutomationId, validCommandHex, validDefinitionJson, validDispatchKey, validEnableInput, validNote, validOptionalAutomationId, validRunInput, validWorkspaceRoot } from "./automation-args.ts";
import { bad } from "./control-args.ts";
import { headersView, type Registrar } from "./conversation-ipc.ts";

const n = (v: bigint | number): number => Number(v);

export function issueView(i: AutomationIssue): AutoIssue {
  return { path: i.path, code: i.code, message: i.message };
}

export function triggerView(t: AutomationTriggerView): AutoTrigger {
  return { id: t.id, kind: t.kind, summary: t.summary, nextDueMs: n(t.nextDueMs) };
}

export function autoView(v: AutomationView): AutoView {
  const e = v.enabled;
  return {
    automationId: v.automationId,
    name: v.name,
    description: v.description,
    currentVersion: v.currentVersion,
    definitionHash: v.definitionHash,
    state: v.state,
    disabledReason: v.disabledReason,
    disabledDetail: v.disabledDetail,
    paused: v.paused,
    enabled: e
      ? { version: e.version, definitionHash: e.definitionHash, effects: e.effects, capabilities: [...e.capabilities], paths: [...e.paths], hosts: [...e.hosts], approver: e.approver, approvedMs: n(e.approvedMs), repoRevision: e.repoRevision, sourceSha256: e.sourceSha256 }
      : null,
    sourceKind: v.sourceKind,
    sourcePath: v.sourcePath,
    workspaceRoot: v.workspaceRoot,
    principal: v.principal,
    triggers: v.triggers.map(triggerView),
    effects: v.effects,
    capabilities: [...v.capabilities],
    paths: [...v.paths],
    hosts: [...v.hosts],
    consecutiveFailures: v.consecutiveFailures,
    lastRunMs: n(v.lastRunMs),
    lastRunStatus: v.lastRunStatus,
    definitionJson: v.definitionJson,
    needsListedApproval: v.needsListedApproval,
    contentChanged: v.contentChanged,
    queued: v.queued,
    active: v.active,
    limitDeadlineMinutes: n(v.limitDeadlineMinutes),
    limitMaxCostMinor: n(v.limitMaxCostMinor),
    limitApprovalWaitMinutes: n(v.limitApprovalWaitMinutes),
  };
}

export function runView(r: AutomationRunView): AutoRun {
  return {
    dispatchKey: r.dispatchKey,
    automationId: r.automationId,
    name: r.name,
    version: r.version,
    triggerId: r.triggerId,
    triggerKind: r.triggerKind,
    eventId: r.eventId,
    status: r.status,
    reason: r.reason,
    detail: r.detail,
    taskId: r.taskId,
    sessionId: r.sessionId,
    principal: r.principal,
    firedMs: n(r.firedMs),
    dispatchedMs: n(r.dispatchedMs),
    finishedMs: n(r.finishedMs),
    costMinor: n(r.costMinor),
    test: r.test,
    catchUp: r.catchUp,
    missed: n(r.missed),
    findings: r.findings,
    outputsJson: r.outputsJson,
    acknowledged: r.acknowledged,
    slotMs: n(r.slotMs),
  };
}

export function attentionView(a: AutomationAttention): AutoAttention {
  return { kind: a.kind, automationId: a.automationId, name: a.name, dispatchKey: a.dispatchKey, taskId: a.taskId, reason: a.reason, action: a.action, atMs: n(a.atMs) };
}

export function listView(l: AutomationList): AutoList {
  return { automations: l.automations.map(autoView), globalPaused: l.globalPaused, attention: l.attention.map(attentionView), nowMs: n(l.nowMs), clock: l.clock };
}

export function detailView(d: AutomationDetail): AutoDetail {
  return {
    view: d.view ? autoView(d.view) : null,
    versions: d.versions.map((v) => ({ version: v.version, definitionHash: v.definitionHash, definitionJson: v.definitionJson, sourceKind: v.sourceKind, sourcePath: v.sourcePath, sourceRevision: v.sourceRevision, createdMs: n(v.createdMs), createdBy: v.createdBy, approved: v.approved })),
  };
}

export function validationView(v: AutomationValidation): AutoValidation {
  return {
    ok: v.ok,
    issues: v.issues.map(issueView),
    name: v.name,
    definitionHash: v.definitionHash,
    canonicalJson: v.canonicalJson,
    effects: v.effects,
    capabilities: [...v.capabilities],
    paths: [...v.paths],
    hosts: [...v.hosts],
    needsListedApproval: v.needsListedApproval,
    triggers: v.triggers.map(triggerView),
  };
}

export function repoView(r: RepositoryAutomations): AutoRepoLoad {
  return { loaded: r.loaded.map(autoView), problems: r.problems.map(issueView) };
}

export function startedView(s: AutomationRunStarted): AutoRunStarted {
  return { dispatchKey: s.dispatchKey, status: s.status, reason: s.reason, detail: s.detail, taskId: s.taskId, wouldSkipByFilter: s.wouldSkipByFilter };
}

export function killView(k: KillReport): AutoKillReport {
  return { cancelledRuns: k.cancelledRuns, droppedQueued: k.droppedQueued, paused: k.paused };
}

/** How many run sessions the agent list reads headers from (the newest runs; the list is a window, not an archive). */
const MAX_RUN_SESSIONS = 40;

export function registerAutomationHandlers(r: Registrar): void {
  r.handle("automations:list", async () => listView(await listAutomations(r.client())));
  r.handle("automations:get", async (...a) => detailView(await getAutomation(r.client(), validAutomationId(a[0]))));
  r.handle("automations:runs", async (...a) => {
    const o = (a[0] ?? {}) as Record<string, unknown>;
    if (typeof o !== "object" || Array.isArray(o)) bad("options must be an object");
    const taskId = o.taskId === undefined || o.taskId === "" ? "" : r.taskId(o.taskId);
    const limit = o.limit === undefined ? 100 : o.limit;
    if (typeof limit !== "number" || !Number.isInteger(limit) || limit < 1 || limit > 500) bad("limit must be 1..500");
    const res = await listAutomationRuns(r.client(), { automationId: validOptionalAutomationId(o.automationId), taskId, limit: limit as number });
    return res.runs.map(runView);
  });
  // The editor's live validation: the Core's parser and validator, nothing stored.
  r.handle("automations:validate", async (...a) => validationView(await validateAutomation(r.client(), validDefinitionJson(a[0]))));
  r.handle("automations:create", async (...a) => autoView(await createAutomation(r.client(), validDefinitionJson(a[0]), validWorkspaceRoot(a[1]), validCommandHex(a[2]))));
  r.handle("automations:update", async (...a) => autoView(await updateAutomation(r.client(), validAutomationId(a[0]), validDefinitionJson(a[1]), validCommandHex(a[2]))));
  r.handle("automations:loadRepository", async (...a) => repoView(await loadRepositoryAutomations(r.client(), validWorkspaceRoot(a[0]))));
  // The owner's approval names exactly what the dialog showed; the Core refuses any difference.
  r.handle("automations:enable", async (...a) => autoView(await enableAutomation(r.client(), validEnableInput(a[0]), validCommandHex(a[1]))));
  r.handle("automations:disable", async (...a) => autoView(await disableAutomation(r.client(), validAutomationId(a[0]), validNote(a[1]))));
  r.handle("automations:pause", async (...a) => {
    const id = validOptionalAutomationId(a[0]);
    if (typeof a[1] !== "boolean") bad("paused must be a boolean");
    const res = await pauseAutomation(r.client(), id, a[1] === true, validNote(a[2]));
    return { view: res.view ? autoView(res.view) : null, list: res.list ? listView(res.list) : null };
  });
  r.handle("automations:kill", async (...a) => killView(await killAutomation(r.client(), validOptionalAutomationId(a[0]), validNote(a[1]))));
  r.handle("automations:run", async (...a) => {
    const input = validRunInput(a[0]);
    return startedView(await runAutomation(r.client(), { ...input }, validCommandHex(a[1])));
  });
  r.handle("automations:ack", async (...a) => {
    await ackAutomationAttention(r.client(), validDispatchKey(a[0]));
    return { acknowledged: true };
  });
  // AUT-E03: a run is its own session, so the desktop session's agent list does not hold the tasks automations created.
  // This reads the headers of the newest runs' sessions, as the Core projects them (origin `automation` is the Core's word).
  r.handle("automations:taskHeaders", async () => {
    const c = r.client();
    const runs = (await listAutomationRuns(c, { limit: 500 })).runs.filter((x) => x.sessionId !== "");
    const sessions: string[] = [];
    for (const run of runs) if (!sessions.includes(run.sessionId) && sessions.length < MAX_RUN_SESSIONS) sessions.push(run.sessionId);
    const out: AgentHeaderView[] = [];
    for (const sid of sessions) {
      try {
        out.push(...headersView(sid, await c.getAgentHeaders(sid, false)).headers);
      } catch {
        // A session the Core can no longer project is simply not listed.
      }
    }
    return out;
  });
}
