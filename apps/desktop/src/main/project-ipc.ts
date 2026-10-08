/**
 * Main's side of projects and the worktree surface (REQ-PX-063, 064, 068): the
 * typed, argument-validated handlers over the Core's project commands, the
 * worktree list, the cleanup and the removal of one worktree, and the
 * apply-back commands. Every handler validates what the renderer sent before it
 * is forwarded (REQ-EV-0103), calls the one client and converts the Core's
 * answer to the plain shapes of shared/project-types.ts. Nothing is decided and
 * nothing is kept here: membership, rollups, a worktree's eligibility, the
 * options a conflict offers and the checkout's bytes are the Core's. Main runs
 * no Git and removes no directory (QUAL-PX-068 dependency scan).
 */
import type { CoreClient } from "@modbit/ide-adapter-core";
import { addProjectMember, applyWorktree, archiveProject, createProject, discardWorktree, getProject, listProjects, listWorktrees, removeProjectMember, removeWorktree, renameProject, runWorktreeCleanup, undoApply } from "@modbit/ide-adapter-core";
import type { ApplyConflictView, ProjectChanged, ProjectList, ProjectMemberView, ProjectView, WorktreeApplyAck, WorktreeCleanupReport, WorktreeList, WorktreeRemoval, WorktreeView } from "@modbit/surface-protocol";
import { APPLY_OPTIONS, PROJECT_COLORS, PROJECT_ICONS, type ApplyAckInfo, type ApplyConflictInfo, type CleanupReportInfo, type ProjectChangedInfo, type ProjectInfo, type ProjectListInfo, type ProjectMemberInfo, type WorktreeInfo, type WorktreeListInfo, type WorktreeRemovalInfo } from "../shared/project-types.ts";
import { bad, validBool, validCommandId } from "./control-args.ts";
import { statusClassOf, type Registrar } from "./conversation-ipc.ts";

const hexOf = (b: Uint8Array | undefined): string => Buffer.from(b ?? []).toString("hex");
const HEX32 = /^[0-9a-f]{32}$/;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

// --------------------------------------------------------------- conversion

function memberInfo(m: ProjectMemberView): ProjectMemberInfo {
  const pr = m.pullRequest;
  return {
    taskId: hexOf(m.taskId?.value),
    sessionId: hexOf(m.sessionId?.value),
    title: m.title,
    statusClass: statusClassOf(m.statusClass),
    statusLabel: m.statusLabel,
    unread: m.unread,
    pendingApproval: m.pendingApproval,
    attentionItems: m.attentionItems,
    taskState: m.taskState,
    archived: m.archived,
    addedAtMs: Number(m.addedAtMs),
    updatedAtMs: Number(m.updatedAtMs),
    pullRequest: pr ? { number: pr.number.toString(), url: pr.url, head: pr.head, state: pr.state, hasCi: pr.hasCi, checksPassed: pr.checksPassed, checksFailed: pr.checksFailed, checksPending: pr.checksPending } : null,
  };
}

export function projectInfo(p: ProjectView): ProjectInfo {
  const r = p.rollup;
  return {
    projectId: hexOf(p.projectId?.value),
    name: p.name,
    color: p.color,
    icon: p.icon,
    workspaceRoot: p.workspaceRoot,
    archived: p.archived,
    createdAtMs: Number(p.createdAtMs),
    updatedAtMs: Number(p.updatedAtMs),
    createdOffset: p.createdOffset.toString(),
    lastOffset: p.lastOffset.toString(),
    rollup: {
      members: r?.members ?? 0,
      byStatus: (r?.byStatus ?? []).map((s) => ({ statusClass: statusClassOf(s.statusClass), label: s.label, count: s.count })),
      attentionTasks: r?.attentionTasks ?? 0,
      attentionItems: r?.attentionItems ?? 0,
      pendingApprovals: r?.pendingApprovals ?? 0,
      unread: r?.unread ?? 0,
      pullRequests: r?.pullRequests ?? 0,
      ciPassing: r?.ciPassing ?? 0,
      ciFailing: r?.ciFailing ?? 0,
      ciPending: r?.ciPending ?? 0,
    },
    members: p.members.map(memberInfo),
  };
}

export const projectListInfo = (l: ProjectList): ProjectListInfo => ({ projects: l.projects.map(projectInfo), lastOffset: l.lastOffset.toString() });
const changedInfo = (c: ProjectChanged): ProjectChangedInfo => ({ project: projectInfo(c.project ?? ({ members: [], createdOffset: 0n, lastOffset: 0n } as unknown as ProjectView)), offset: c.offset.toString() });

export function worktreeInfo(w: WorktreeView): WorktreeInfo {
  return {
    worktreeId: w.worktreeId,
    taskId: hexOf(w.taskId?.value),
    kind: w.kind,
    path: w.path,
    originRoot: w.originRoot,
    branch: w.branch,
    baseRevision: w.baseRevision,
    state: w.state,
    disposition: w.disposition,
    dirty: w.dirty,
    unapplied: w.unapplied,
    changedFiles: w.changedFiles,
    bytes: w.bytes.toString(),
    createdAtMs: Number(w.createdAtMs),
    lastActivityMs: Number(w.lastActivityMs),
    taskRunning: w.taskRunning,
    orphan: w.orphan,
    protected: w.protected,
    removable: w.removable,
    removableReason: w.removableReason,
    setupStatus: w.setupStatus,
    setupDetail: w.setupDetail,
    neutralized: w.neutralized,
  };
}

export function cleanupInfo(r: WorktreeCleanupReport): CleanupReportInfo {
  return {
    runId: r.runId,
    owner: r.owner,
    reason: r.reason,
    status: r.status,
    skippedReason: r.skippedReason,
    scanned: r.scanned,
    removed: r.removed,
    bytesFreed: r.bytesFreed.toString(),
    errors: r.errors,
    removedIds: r.removedIds,
    kept: r.kept.map((k) => ({ worktreeId: k.worktreeId, why: k.why })),
    attention: r.attention,
    startedAtMs: Number(r.startedAtMs),
    finishedAtMs: Number(r.finishedAtMs),
    remaining: r.remaining,
    remainingBytes: r.remainingBytes.toString(),
    holderOwner: r.holderOwner,
    holderReason: r.holderReason,
  };
}

export function worktreeListInfo(l: WorktreeList): WorktreeListInfo {
  const p = l.policy;
  return {
    worktrees: l.worktrees.map(worktreeInfo),
    count: l.count,
    totalBytes: l.totalBytes.toString(),
    policy: p ? { maxWorktrees: p.maxWorktrees, maxBytes: p.maxBytes.toString(), protectMs: Number(p.protectMs), hysteresisPercent: p.hysteresisPercent } : null,
    lastCleanup: l.lastCleanup ? cleanupInfo(l.lastCleanup) : null,
    attention: l.attention,
    nextCleanupAtMs: Number(l.nextCleanupAtMs),
    lastCompletedAtMs: Number(l.lastCompletedAtMs),
  };
}

export const removalInfo = (r: WorktreeRemoval): WorktreeRemovalInfo => ({ worktreeId: r.worktreeId, removed: r.removed, dryRun: r.dryRun, branch: r.branch, path: r.path, bytes: r.bytes.toString(), reason: r.reason, loses: r.loses, offset: r.offset.toString() });

export function conflictInfo(v: ApplyConflictView): ApplyConflictInfo {
  return {
    planDigest: v.planDigest,
    candidateTree: v.candidateTree,
    checkoutHead: v.checkoutHead,
    candidateRevision: v.candidateRevision.toString(),
    paths: v.paths.map((p) => ({ path: p.path, change: p.change, state: p.state, conflict: p.conflict, protected: p.protected, markersAvailable: p.markersAvailable })),
    conflictingPaths: v.conflictingPaths,
    protectedPaths: v.protectedPaths,
    dirtyPaths: v.dirtyPaths,
    options: v.options.map((o) => ({ option: o.option, destructive: o.destructive, needsConfirmation: o.needsConfirmation, confirmPaths: o.confirmPaths, available: o.available, whyUnavailable: o.whyUnavailable })),
    defaultOption: v.defaultOption,
    rememberedOption: v.rememberedOption,
  };
}

export function applyAckInfo(a: WorktreeApplyAck): ApplyAckInfo {
  return {
    status: a.status,
    applyId: a.applyId,
    approvalId: a.approvalId,
    intentHash: a.intentHash,
    planDigest: a.planDigest,
    candidateRevision: a.candidateRevision.toString(),
    candidateTree: a.candidateTree,
    checkoutHeadBefore: a.checkoutHeadBefore,
    conflict: a.conflict ? conflictInfo(a.conflict) : null,
    appliedPaths: a.appliedPaths,
    unresolvedPaths: a.unresolvedPaths,
    effectReceiptIds: a.effectReceiptIds,
    detail: a.detail,
    code: a.code,
    option: a.option,
    stashRef: a.stashRef,
    replayed: a.replayed,
    divergentPaths: a.divergentPaths,
  };
}

// --------------------------------------------------------------- validation

export function validProjectId(v: unknown): string {
  const s = typeof v === "string" ? v.replace(/-/g, "").toLowerCase() : "";
  if (!HEX32.test(s)) return bad("projectId must be a project id");
  return s;
}

/** A project name as typed: 1..80 characters, no control characters. The Core trims and refuses the rest (taken names, the limit). */
export function validProjectName(v: unknown, optional = false): string {
  if (optional && (v === undefined || v === null || v === "")) return "";
  if (typeof v !== "string" || v.trim().length === 0 || v.length > 200 || CONTROL.test(v)) return bad("name must be 1..80 printable characters");
  return v;
}

function validToken(v: unknown, allowed: readonly string[], what: string): string {
  if (v === undefined || v === null || v === "") return "";
  if (typeof v !== "string" || !allowed.includes(v)) return bad(`${what} must be one of ${allowed.join(", ")}`);
  return v;
}

export function validWorkspaceRoot(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 4096 || CONTROL.test(v)) return bad("workspaceRoot must be a path");
  return v;
}

export function validWorktreeId(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 4096 || CONTROL.test(v)) return bad("worktreeId must be a worktree id");
  return v;
}

function validObject(v: unknown, what: string): Record<string, unknown> {
  if (v === undefined || v === null) return {};
  if (typeof v !== "object" || Array.isArray(v)) return bad(`${what} must be an object`);
  return v as Record<string, unknown>;
}

/** Paths the person typed to confirm an overwrite: bounded, printable, and never absolute or leaving the checkout. The Core compares them with the files it will replace. */
export function validConfirmPaths(v: unknown): string[] {
  if (v === undefined || v === null) return [];
  if (!Array.isArray(v) || v.length > 2000) return bad("confirmPaths must be a list of at most 2000 paths");
  return v.map((p) => {
    if (typeof p !== "string" || p.length === 0 || p.length > 4096 || CONTROL.test(p)) return bad("a confirmed path must be a printable path");
    return p;
  });
}

export function validApplyInput(v: unknown): { option: string; confirmPaths: string[]; remember: boolean; expectedCandidateRevision: bigint; expectedPlanDigest: string } {
  const o = validObject(v, "apply options");
  const option = o.option === undefined || o.option === null || o.option === "" ? "" : validToken(o.option, APPLY_OPTIONS, "option");
  const rev = o.expectedCandidateRevision;
  if (rev !== undefined && rev !== null && rev !== "" && (typeof rev !== "string" || !/^\d{1,18}$/.test(rev))) bad("expectedCandidateRevision must be a decimal revision");
  const digest = o.expectedPlanDigest;
  if (digest !== undefined && digest !== null && digest !== "" && (typeof digest !== "string" || !/^[0-9a-f]{64}$/.test(digest))) bad("expectedPlanDigest must be the 64-hex-digit digest the modal showed");
  return {
    option,
    confirmPaths: validConfirmPaths(o.confirmPaths),
    remember: o.remember === undefined ? false : validBool(o.remember, "remember"),
    expectedCandidateRevision: typeof rev === "string" && rev !== "" ? BigInt(rev) : 0n,
    expectedPlanDigest: typeof digest === "string" ? digest : "",
  };
}

export function validProjectInput(v: unknown): { name: string; color: string; icon: string; workspaceRoot: string } {
  const o = validObject(v, "project");
  return { name: validProjectName(o.name), color: validToken(o.color, PROJECT_COLORS, "color"), icon: validToken(o.icon, PROJECT_ICONS, "icon"), workspaceRoot: validWorkspaceRoot(o.workspaceRoot) };
}

export function validProjectPatch(v: unknown): { name: string; color: string; icon: string } {
  const o = validObject(v, "patch");
  return { name: validProjectName(o.name, true), color: validToken(o.color, PROJECT_COLORS, "color"), icon: validToken(o.icon, PROJECT_ICONS, "icon") };
}

// ------------------------------------------------------------- the handlers

/** The commands of projects and the worktree surface. */
export function registerProjectHandlers(r: Registrar): void {
  r.handle("projects:list", async (...a) => {
    const o = validObject(a[0], "options");
    if (o.includeArchived !== undefined && typeof o.includeArchived !== "boolean") bad("includeArchived must be a boolean");
    const workspaceRoot = o.workspaceRoot === undefined || o.workspaceRoot === "" ? undefined : validWorkspaceRoot(o.workspaceRoot);
    const includeArchived = o.includeArchived === true;
    return projectListInfo(await listProjects(r.client(), { ...(workspaceRoot ? { workspaceRoot } : {}), includeArchived }));
  });
  r.handle("projects:get", async (...a) => {
    const id = validProjectId(a[0]);
    return changedInfo(await getProject(r.client(), id));
  });
  r.handle("projects:create", async (...a) => {
    const sid = r.sessionId(a[0]);
    const input = validProjectInput(a[1]);
    const commandId = validCommandId(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    return changedInfo(await createProject(c, sid, input, commandId));
  });
  r.handle("projects:rename", async (...a) => {
    const sid = r.sessionId(a[0]);
    const id = validProjectId(a[1]);
    const patch = validProjectPatch(a[2]);
    const commandId = validCommandId(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    return changedInfo(await renameProject(c, sid, id, patch, commandId));
  });
  r.handle("projects:archive", async (...a) => {
    const sid = r.sessionId(a[0]);
    const id = validProjectId(a[1]);
    const archived = validBool(a[2], "archived");
    const commandId = validCommandId(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    return changedInfo(await archiveProject(c, sid, id, archived, commandId));
  });
  r.handle("projects:addMember", async (...a) => {
    const sid = r.sessionId(a[0]);
    const id = validProjectId(a[1]);
    const tid = r.taskId(a[2]);
    const commandId = validCommandId(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    return changedInfo(await addProjectMember(c, sid, id, tid, commandId));
  });
  r.handle("projects:removeMember", async (...a) => {
    const sid = r.sessionId(a[0]);
    const id = validProjectId(a[1]);
    const tid = r.taskId(a[2]);
    const commandId = validCommandId(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    return changedInfo(await removeProjectMember(c, sid, id, tid, commandId));
  });

  r.handle("worktrees:list", async (...a) => {
    const o = validObject(a[0], "options");
    const sessionId = o.sessionId === undefined || o.sessionId === "" ? undefined : r.sessionId(o.sessionId);
    const taskId = o.taskId === undefined || o.taskId === "" ? undefined : r.taskId(o.taskId);
    const c = r.client();
    return worktreeListInfo(await listWorktrees(c, { ...(sessionId ? { sessionId } : {}), ...(taskId ? { taskId } : {}) }));
  });
  r.handle("worktrees:remove", async (...a) => {
    const sid = r.sessionId(a[0]);
    const id = validWorktreeId(a[1]);
    const dryRun = validBool(a[2], "dryRun");
    const commandId = validCommandId(a[3]);
    const c = r.client();
    await r.lease(c, sid);
    return removalInfo(await removeWorktree(c, sid, id, dryRun, commandId));
  });
  r.handle("worktrees:cleanup", async (...a) => {
    const sid = r.sessionId(a[0]);
    const o = validObject(a[1], "options");
    if (o.dryRun !== undefined && typeof o.dryRun !== "boolean") bad("dryRun must be a boolean");
    const c = r.client();
    await r.lease(c, sid);
    return cleanupInfo(await runWorktreeCleanup(c, sid, { dryRun: o.dryRun === true }));
  });
  r.handle("worktrees:apply", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const input = validApplyInput(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    return applyAckInfo(await applyWorktree(c, sid, tid, input));
  });
  r.handle("worktrees:undo", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const applyId = a[2] === undefined || a[2] === null || a[2] === "" ? "" : typeof a[2] === "string" && a[2].length <= 128 && !CONTROL.test(a[2]) ? a[2] : bad("applyId must be an apply id");
    const c = r.client();
    await r.lease(c, sid);
    return applyAckInfo(await undoApply(c, sid, tid, applyId));
  });
  r.handle("worktrees:discard", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const reason = typeof a[2] === "string" && a[2].length <= 2000 && !CONTROL.test(a[2].replace(/\n/g, " ")) ? a[2] : a[2] === undefined || a[2] === null ? "" : bad("reason must be text");
    const confirm = typeof a[3] === "string" && a[3].length > 0 && a[3].length <= 512 && !CONTROL.test(a[3]) ? a[3] : bad("confirm must be the branch name typed");
    const c = r.client();
    await r.lease(c, sid);
    const res = await discardWorktree(c, sid, tid, reason, confirm);
    return { worktreeId: res.worktreeId, disposition: res.disposition, offset: res.offset.toString() };
  });
}
