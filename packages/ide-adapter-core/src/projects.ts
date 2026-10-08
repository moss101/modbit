/**
 * Typed commands of the projects and the worktree surface (REQ-PX-063,
 * REQ-PX-064, REQ-PX-068), over the one SurfaceProtocol client. Every function
 * is a thin typed call: the Core decides (which task may join which project,
 * which worktree may be removed, what an apply does to the checkout) and these
 * return its answer unchanged, or throw its typed refusal. A mutation takes the
 * session lease the client holds, so a client that does not hold it is
 * refused by the Core, never by this file.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  AddProjectMemberSchema,
  ApplyWorktreeSchema,
  ArchiveProjectSchema,
  CreateProjectSchema,
  DiscardWorktreeSchema,
  GetProjectSchema,
  ListProjectsSchema,
  ListWorktreesSchema,
  ProjectChangedSchema,
  ProjectListSchema,
  RemoveProjectMemberSchema,
  RemoveWorktreeSchema,
  RenameProjectSchema,
  RunWorktreeCleanupSchema,
  UndoApplySchema,
  WorktreeApplyAckSchema,
  WorktreeCleanupReportSchema,
  WorktreeDiscardedSchema,
  WorktreeListSchema,
  WorktreeRemovalSchema,
  type ProjectChanged,
  type ProjectList,
  type WorktreeApplyAck,
  type WorktreeCleanupReport,
  type WorktreeDiscarded,
  type WorktreeList,
  type WorktreeRemoval,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });
const lease = (c: CoreClient, sessionId: string): bigint | undefined => c.leaseGeneration(sessionId);

// ------------------------------------------------------------------ projects

export async function listProjects(c: CoreClient, opts: { workspaceRoot?: string; includeArchived?: boolean } = {}): Promise<ProjectList> {
  const payload = toBinary(ListProjectsSchema, create(ListProjectsSchema, { workspaceRoot: opts.workspaceRoot ?? "", includeArchived: opts.includeArchived ?? false }));
  return fromBinary(ProjectListSchema, (await c.command("ListProjects", payload)).result);
}

export async function getProject(c: CoreClient, projectId: string): Promise<ProjectChanged> {
  const payload = toBinary(GetProjectSchema, create(GetProjectSchema, { projectId: id(projectId) }));
  return fromBinary(ProjectChangedSchema, (await c.command("GetProject", payload)).result);
}

export async function createProject(c: CoreClient, sessionId: string, p: { name: string; color?: string; icon?: string; workspaceRoot: string }, commandId?: Uint8Array): Promise<ProjectChanged> {
  const payload = toBinary(CreateProjectSchema, create(CreateProjectSchema, { sessionId: id(sessionId), name: p.name, color: p.color ?? "", icon: p.icon ?? "", workspaceRoot: p.workspaceRoot }));
  return fromBinary(ProjectChangedSchema, (await c.command("CreateProject", payload, commandId, lease(c, sessionId))).result);
}

export async function renameProject(c: CoreClient, sessionId: string, projectId: string, p: { name?: string; color?: string; icon?: string }, commandId?: Uint8Array): Promise<ProjectChanged> {
  const payload = toBinary(RenameProjectSchema, create(RenameProjectSchema, { sessionId: id(sessionId), projectId: id(projectId), name: p.name ?? "", color: p.color ?? "", icon: p.icon ?? "" }));
  return fromBinary(ProjectChangedSchema, (await c.command("RenameProject", payload, commandId, lease(c, sessionId))).result);
}

export async function archiveProject(c: CoreClient, sessionId: string, projectId: string, archived: boolean, commandId?: Uint8Array): Promise<ProjectChanged> {
  const payload = toBinary(ArchiveProjectSchema, create(ArchiveProjectSchema, { sessionId: id(sessionId), projectId: id(projectId), archived }));
  return fromBinary(ProjectChangedSchema, (await c.command("ArchiveProject", payload, commandId, lease(c, sessionId))).result);
}

export async function addProjectMember(c: CoreClient, sessionId: string, projectId: string, taskId: string, commandId?: Uint8Array): Promise<ProjectChanged> {
  const payload = toBinary(AddProjectMemberSchema, create(AddProjectMemberSchema, { sessionId: id(sessionId), projectId: id(projectId), taskId: id(taskId) }));
  return fromBinary(ProjectChangedSchema, (await c.command("AddProjectMember", payload, commandId, lease(c, sessionId))).result);
}

export async function removeProjectMember(c: CoreClient, sessionId: string, projectId: string, taskId: string, commandId?: Uint8Array): Promise<ProjectChanged> {
  const payload = toBinary(RemoveProjectMemberSchema, create(RemoveProjectMemberSchema, { sessionId: id(sessionId), projectId: id(projectId), taskId: id(taskId) }));
  return fromBinary(ProjectChangedSchema, (await c.command("RemoveProjectMember", payload, commandId, lease(c, sessionId))).result);
}

// ----------------------------------------------------------------- worktrees

export async function listWorktrees(c: CoreClient, opts: { sessionId?: string; taskId?: string } = {}): Promise<WorktreeList> {
  const payload = toBinary(ListWorktreesSchema, create(ListWorktreesSchema, { sessionId: opts.sessionId ? id(opts.sessionId) : undefined, taskId: opts.taskId ? id(opts.taskId) : undefined }));
  return fromBinary(WorktreeListSchema, (await c.command("ListWorktrees", payload)).result);
}

/** Remove the one worktree the person chose; with `dryRun` the Core says what would be lost and removes nothing. */
export async function removeWorktree(c: CoreClient, sessionId: string, worktreeId: string, dryRun: boolean, commandId?: Uint8Array): Promise<WorktreeRemoval> {
  const payload = toBinary(RemoveWorktreeSchema, create(RemoveWorktreeSchema, { sessionId: id(sessionId), worktreeId, dryRun }));
  return fromBinary(WorktreeRemovalSchema, (await c.command("RemoveWorktree", payload, commandId, dryRun ? undefined : lease(c, sessionId))).result);
}

export async function runWorktreeCleanup(c: CoreClient, sessionId: string, opts: { reason?: string; dryRun?: boolean } = {}): Promise<WorktreeCleanupReport> {
  const payload = toBinary(RunWorktreeCleanupSchema, create(RunWorktreeCleanupSchema, { reason: opts.reason ?? "requested from the desktop", dryRun: opts.dryRun ?? false }));
  return fromBinary(WorktreeCleanupReportSchema, (await c.command("RunWorktreeCleanup", payload, undefined, lease(c, sessionId))).result);
}

export interface ApplyInput {
  /** "" and CANCEL do nothing about conflicts; MERGE_MANUALLY | STASH | OVERWRITE | FULL_OVERWRITE | UNDO_AND_APPLY act on them. */
  option?: string;
  confirmPaths?: string[];
  remember?: boolean;
  expectedCandidateRevision?: bigint;
  expectedPlanDigest?: string;
}

export async function applyWorktree(c: CoreClient, sessionId: string, taskId: string, a: ApplyInput): Promise<WorktreeApplyAck> {
  const payload = toBinary(
    ApplyWorktreeSchema,
    create(ApplyWorktreeSchema, { taskId: id(taskId), option: a.option ?? "", confirmPaths: a.confirmPaths ?? [], remember: a.remember ?? false, expectedCandidateRevision: a.expectedCandidateRevision ?? 0n, expectedPlanDigest: a.expectedPlanDigest ?? "" }),
  );
  return fromBinary(WorktreeApplyAckSchema, (await c.command("ApplyWorktree", payload, undefined, lease(c, sessionId))).result);
}

export async function undoApply(c: CoreClient, sessionId: string, taskId: string, applyId = ""): Promise<WorktreeApplyAck> {
  const payload = toBinary(UndoApplySchema, create(UndoApplySchema, { taskId: id(taskId), applyId }));
  return fromBinary(WorktreeApplyAckSchema, (await c.command("UndoApply", payload, undefined, lease(c, sessionId))).result);
}

export async function discardWorktree(c: CoreClient, sessionId: string, taskId: string, reason: string, confirm: string): Promise<WorktreeDiscarded> {
  const payload = toBinary(DiscardWorktreeSchema, create(DiscardWorktreeSchema, { taskId: id(taskId), reason, confirm }));
  return fromBinary(WorktreeDiscardedSchema, (await c.command("DiscardWorktree", payload, undefined, lease(c, sessionId))).result);
}
