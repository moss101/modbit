/**
 * The worktree list's pure view model (REQ-PX-068): which words a worktree's
 * state gets, how sizes and ages read, how the list groups by repository, and
 * what a typed refusal of a removal means. Nothing here decides whether a
 * worktree may be removed: that is the Core's eligibility (`removable`,
 * `removableReason`) and, when it is asked, the Core's typed refusal. This file
 * only chooses words for facts the Core gave.
 */
import type { WorktreeInfo } from "../../shared/project-types.ts";

export type StateId = "running" | "dirty" | "unapplied" | "retained" | "removable" | "orphan" | "missing";

export interface StateBadge {
  id: StateId;
  label: string;
  /** Why, in the Core's words where it has some. */
  detail: string;
  tone: "ok" | "warn" | "info" | "danger";
}

const baseName = (p: string): string => p.split(/[\\/]+/).filter(Boolean).pop() ?? p;

/**
 * The states a worktree is in, from the Core's own booleans: a running task, a
 * dirty tree, a result no decision covers, a worktree no task owns, a record
 * whose directory is gone. A worktree with none of those is `removable` when
 * the Core says so, else `retained` (the Core keeps it: young, or undecided).
 */
export function worktreeStates(w: WorktreeInfo): StateBadge[] {
  if (w.state === "MISSING") return [{ id: "missing", label: "Missing", detail: "its directory is gone from the disk", tone: "danger" }];
  const out: StateBadge[] = [];
  if (w.taskRunning) out.push({ id: "running", label: "Running", detail: "its task has not ended", tone: "info" });
  if (w.dirty) out.push({ id: "dirty", label: "Dirty", detail: `${w.changedFiles} changed ${w.changedFiles === 1 ? "file" : "files"} not committed`, tone: "warn" });
  if (w.unapplied) out.push({ id: "unapplied", label: "Unapplied", detail: "it holds a result that was not applied, merged or discarded", tone: "warn" });
  if (w.orphan) out.push({ id: "orphan", label: "Orphan", detail: "no task of this Core owns it", tone: "warn" });
  if (out.length === 0) out.push(w.removable ? { id: "removable", label: "Removable", detail: w.removableReason, tone: "ok" } : { id: "retained", label: "Retained", detail: w.removableReason, tone: "info" });
  return out;
}

/** The disposition as one word, or "" when the worktree has none. */
export const dispositionWord = (d: string): string => (d ? d.charAt(0) + d.slice(1).toLowerCase() : "");

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;
/** A decimal byte count (64-bit on the wire) as words. */
export function sizeWords(bytes: string): string {
  let n = Number(bytes);
  if (!Number.isFinite(n) || n < 0) return "unknown size";
  let i = 0;
  while (n >= 1024 && i < UNITS.length - 1) {
    n /= 1024;
    i++;
  }
  return `${i === 0 ? n : n >= 10 ? Math.round(n) : n.toFixed(1)} ${UNITS[i]}`;
}

/** A duration since `thenMs` as the largest whole unit; "just now" under a minute. */
export function ageWords(thenMs: number, nowMs: number): string {
  if (!thenMs) return "unknown age";
  const s = Math.max(0, Math.floor((nowMs - thenMs) / 1000));
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h} h`;
  return `${Math.floor(h / 24)} days`;
}

export interface RepoGroup {
  /** The checkout the worktrees were made from; "" for worktrees no task of this Core made. */
  origin: string;
  label: string;
  items: WorktreeInfo[];
}

/** Worktrees grouped by the checkout they came from, newest first inside a group; orphans, which have no origin, last. */
export function groupByRepository(ws: readonly WorktreeInfo[]): RepoGroup[] {
  const groups = new Map<string, WorktreeInfo[]>();
  for (const w of ws) groups.set(w.originRoot, [...(groups.get(w.originRoot) ?? []), w]);
  return [...groups.entries()]
    .map(([origin, items]) => ({ origin, label: origin ? baseName(origin) : "No source repository", items: [...items].sort((a, b) => b.createdAtMs - a.createdAtMs || (a.worktreeId < b.worktreeId ? -1 : 1)) }))
    .sort((a, b) => (a.origin === "" ? 1 : b.origin === "" ? -1 : a.label.localeCompare(b.label) || (a.origin < b.origin ? -1 : 1)));
}

/** What the person can do next after a typed refusal of a removal; the Core's own sentence is shown beside it. */
export const REMOVAL_ADVICE: Record<string, string> = {
  TASK_RUNNING: "Wait for the task to end, or stop it, before removing its worktree.",
  WORKTREE_PROTECTED: "A new worktree is kept for a short while; try again later.",
  WORKTREE_NEEDS_DECISION: "Apply its result to your checkout, or discard it, and then it can be removed.",
  WORKTREE_NOT_REMOVABLE: "The Core keeps this worktree for the reason above.",
  UNKNOWN_WORKTREE: "The list is out of date; it has been read again.",
  CLEANUP_LEASE_HELD: "A cleanup is running; try again when it is done.",
  WORKTREE_REMOVE_FAILED: "Nothing else was changed. Close anything using the folder and try again.",
};

/** How the last cleanup is said: a run that did not happen is "Skipped", never "completed". */
export function reportWords(r: { status: string; scanned: number; removed: number; bytesFreed: string; skippedReason: string; holderOwner: string }): string {
  if (r.status === "SKIPPED") return `Skipped: ${r.skippedReason || `another cleanup held the lease (${r.holderOwner})`}`;
  const what = `${r.removed} removed of ${r.scanned} looked at, ${sizeWords(r.bytesFreed)} freed`;
  return r.status === "DRY_RUN" ? `Dry run: ${what} (nothing was removed)` : `Completed: ${what}`;
}
