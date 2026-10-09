/**
 * The plain shapes projects and worktrees cross the preload bridge as
 * (REQ-PX-063, 064, 068). Main converts the Core's protobuf messages into
 * these (64-bit counts as decimal strings, ids as hex, enums as their stable
 * names); the renderer shows them and decides none of it: which tasks are in
 * a project, how many of them need attention, whether a worktree may be
 * removed and what an apply will do are the Core's answers (docs/81).
 */
import type { StatusClass } from "./conversation-types.ts";

/** The palette roles a project may carry (design tokens, PX-044); the Core validates the same set. */
export const PROJECT_COLORS = ["accent", "ok", "warn", "danger", "info", "modeAsk", "modePlan", "modeDebug"] as const;
export type ProjectColor = (typeof PROJECT_COLORS)[number];
/** The icons a project may carry; the Core validates the same set. */
export const PROJECT_ICONS = ["folder", "star", "flag", "bolt", "book", "bug", "rocket", "leaf"] as const;
export type ProjectIcon = (typeof PROJECT_ICONS)[number];

export interface ProjectPullRequestInfo {
  number: string;
  url: string;
  head: string;
  state: string;
  hasCi: boolean;
  checksPassed: number;
  checksFailed: number;
  checksPending: number;
}

export interface ProjectMemberInfo {
  taskId: string;
  sessionId: string;
  title: string;
  statusClass: StatusClass;
  statusLabel: string;
  unread: boolean;
  pendingApproval: boolean;
  attentionItems: number;
  taskState: string;
  archived: boolean;
  addedAtMs: number;
  updatedAtMs: number;
  pullRequest: ProjectPullRequestInfo | null;
}

export interface ProjectRollupInfo {
  members: number;
  byStatus: { statusClass: StatusClass; label: string; count: number }[];
  attentionTasks: number;
  attentionItems: number;
  pendingApprovals: number;
  unread: number;
  pullRequests: number;
  ciPassing: number;
  ciFailing: number;
  ciPending: number;
}

export interface ProjectInfo {
  projectId: string;
  name: string;
  color: string;
  icon: string;
  workspaceRoot: string;
  archived: boolean;
  createdAtMs: number;
  updatedAtMs: number;
  createdOffset: string;
  lastOffset: string;
  rollup: ProjectRollupInfo;
  members: ProjectMemberInfo[];
}

export interface ProjectListInfo {
  projects: ProjectInfo[];
  lastOffset: string;
}

/** What a project command answered: the project as it is now and where the change was recorded ("0": nothing was). */
export interface ProjectChangedInfo {
  project: ProjectInfo;
  offset: string;
}

export interface ProjectInput {
  name: string;
  color?: string;
  icon?: string;
  workspaceRoot: string;
}

export interface ProjectPatch {
  name?: string;
  color?: string;
  icon?: string;
}

// ----------------------------------------------------------------- worktrees

export interface WorktreeInfo {
  worktreeId: string;
  taskId: string;
  kind: string;
  path: string;
  originRoot: string;
  branch: string;
  baseRevision: string;
  /** ACTIVE | MISSING. */
  state: string;
  /** "" | APPLIED | MERGED | DISCARDED | EXPORTED. */
  disposition: string;
  dirty: boolean;
  unapplied: boolean;
  changedFiles: number;
  bytes: string;
  createdAtMs: number;
  lastActivityMs: number;
  taskRunning: boolean;
  orphan: boolean;
  protected: boolean;
  removable: boolean;
  removableReason: string;
  setupStatus: string;
  setupDetail: string;
  neutralized: string[];
}

export interface WorktreePolicyInfo {
  maxWorktrees: number;
  maxBytes: string;
  protectMs: number;
  hysteresisPercent: number;
}

export interface CleanupReportInfo {
  runId: string;
  owner: string;
  reason: string;
  /** COMPLETED | SKIPPED | DRY_RUN. */
  status: string;
  skippedReason: string;
  scanned: number;
  removed: number;
  bytesFreed: string;
  errors: string[];
  removedIds: string[];
  kept: { worktreeId: string; why: string }[];
  attention: string[];
  startedAtMs: number;
  finishedAtMs: number;
  remaining: number;
  remainingBytes: string;
  holderOwner: string;
  holderReason: string;
}

export interface WorktreeListInfo {
  worktrees: WorktreeInfo[];
  count: number;
  totalBytes: string;
  policy: WorktreePolicyInfo | null;
  lastCleanup: CleanupReportInfo | null;
  attention: string[];
  nextCleanupAtMs: number;
  lastCompletedAtMs: number;
}

export interface WorktreeRemovalInfo {
  worktreeId: string;
  removed: boolean;
  dryRun: boolean;
  branch: string;
  path: string;
  bytes: string;
  reason: string;
  loses: string[];
  offset: string;
}

// --------------------------------------------------------------- apply-back

/** The options the Core can offer on a conflict, in its names. Cancel is always the default. */
export const APPLY_OPTIONS = ["CANCEL", "MERGE_MANUALLY", "STASH", "OVERWRITE", "FULL_OVERWRITE", "UNDO_AND_APPLY"] as const;
export type ApplyOptionId = (typeof APPLY_OPTIONS)[number];

export interface ApplyPathInfo {
  path: string;
  /** ADD | MODIFY | DELETE. */
  change: string;
  /** CLEAN | ALREADY_APPLIED | AUTO_MERGE | CONFLICT | UNSUPPORTED. */
  state: string;
  conflict: string;
  protected: boolean;
  markersAvailable: boolean;
}

export interface ApplyOptionInfo {
  option: string;
  destructive: boolean;
  needsConfirmation: boolean;
  confirmPaths: string[];
  available: boolean;
  whyUnavailable: string;
}

export interface ApplyConflictInfo {
  planDigest: string;
  candidateTree: string;
  checkoutHead: string;
  candidateRevision: string;
  paths: ApplyPathInfo[];
  conflictingPaths: string[];
  protectedPaths: string[];
  dirtyPaths: string[];
  options: ApplyOptionInfo[];
  defaultOption: string;
  rememberedOption: string;
}

export interface ApplyAckInfo {
  /** APPROVAL_PENDING | APPLIED | CONFLICT | STALE | DENIED | CANCELLED | NOTHING_TO_APPLY | UNDONE | UNDO_CONFLICT. */
  status: string;
  applyId: string;
  approvalId: string;
  intentHash: string;
  planDigest: string;
  candidateRevision: string;
  candidateTree: string;
  checkoutHeadBefore: string;
  conflict: ApplyConflictInfo | null;
  appliedPaths: string[];
  unresolvedPaths: string[];
  effectReceiptIds: string[];
  detail: string;
  code: string;
  option: string;
  stashRef: string;
  replayed: boolean;
  divergentPaths: string[];
}

export interface ApplyInputArgs {
  option?: string;
  confirmPaths?: string[];
  remember?: boolean;
  expectedCandidateRevision?: string;
  expectedPlanDigest?: string;
}
