/**
 * The Review page's words (REQ-PX-208, DR-PX-2026-11-11-021 MWS-14), pure: which tasks have changes to review, and what the
 * Core recorded about the chosen one (its change set, its findings and people's comments). Every fact is the Core's; nothing is
 * computed that the Core did not say. Two limits are the Core's, not this page's:
 *  - the Core has no read of a pull request's state (PX-211): a task is under "Pull requests" only when the Core answered this
 *    window that it opened or updated one;
 *  - the isolated reviewer's findings are not in the review bundle one by one (PX-214): the findings here are the ones the bundle
 *    carries (failed checks, broken change rules, the agent's own self-review, the forge's failed checks).
 */
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import type { PullRequestAckView, ReviewBundleView } from "../../preload/preload.ts";

export interface ReviewRow {
  taskId: string;
  title: string;
  /** Where the change is: the task's own worktree or the folder's name. */
  where: string;
  state: string;
  files: number;
  added: number;
  removed: number;
  /** The request the Core answered this window with; null for local changes. */
  pr: PullRequestAckView | null;
}

/** A task state in plain words. */
export function stateWords(state: string): string {
  const known: Record<string, string> = { ReadyForReview: "Ready for review", Running: "Running", Waiting: "Waiting for you", Queued: "Not started", Completed: "Finished", Failed: "Failed", Cancelled: "Cancelled", Paused: "Paused", Planning: "Planning", Verifying: "Checking" };
  if (known[state]) return known[state]!;
  const words = state.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

const baseName = (path: string): string => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;

/** Whether a task has changes to review: the Core counts changed files, or the task waits for its review. */
export const hasChanges = (h: AgentHeaderView): boolean => !h.archived && (h.filesChanged > 0 || h.taskState === "ReadyForReview");

/** The page's two lists, newest first: tasks with a pull request this window knows of, and the local changes not in one. */
export function reviewRows(headers: readonly AgentHeaderView[], requestOf: (taskId: string) => PullRequestAckView | null): { pulls: ReviewRow[]; local: ReviewRow[] } {
  const rows = headers
    .filter(hasChanges)
    .slice()
    .sort((a, b) => b.updatedAtMs - a.updatedAtMs)
    .map(
      (h): ReviewRow => ({
        taskId: h.taskId,
        title: h.title || "Untitled task",
        where: h.checkoutRoot ? "its own worktree" : baseName(h.workspaceRoot) || "this computer",
        state: stateWords(h.taskState),
        files: h.filesChanged,
        added: h.linesAdded,
        removed: h.linesRemoved,
        pr: requestOf(h.taskId),
      }),
    );
  return { pulls: rows.filter((r) => r.pr !== null), local: rows.filter((r) => r.pr === null) };
}

export interface ChangedFile {
  path: string;
  status: string;
  added: number;
  removed: number;
}

const FILE_STATUS: Record<string, string> = { A: "added", D: "deleted", R: "renamed", M: "changed" };

/** The change set as the bundle has it: each file, what happened to it and its lines added and removed. */
export function changeSet(b: ReviewBundleView): ChangedFile[] {
  return b.files.map((f) => ({
    path: f.path,
    status: f.binary ? "binary" : (FILE_STATUS[f.status] ?? "changed"),
    added: f.hunks.reduce((n, h) => n + h.lines.filter((l) => l.startsWith("+")).length, 0),
    removed: f.hunks.reduce((n, h) => n + h.lines.filter((l) => l.startsWith("-")).length, 0),
  }));
}

export type FindingKind = "check" | "rule" | "self" | "forge";

export interface Finding {
  kind: FindingKind;
  /** The finding's label, in a word or two. */
  label: string;
  text: string;
  /** The file it is about, when the Core says. */
  path: string;
  /** False for a self-review item the agent marked resolved. */
  open: boolean;
}

const LABEL: Record<FindingKind, string> = { check: "Check", rule: "Change rule", self: "Self-review", forge: "Forge check" };

/** The agent's own self-review items, from the JSON the Core stored with its completion (`findings: [{ text, resolved }]`). */
function selfReview(json: string): Finding[] {
  if (!json) return [];
  let v: unknown;
  try {
    v = JSON.parse(json);
  } catch {
    return [];
  }
  const o = (typeof v === "object" && v !== null ? (v as Record<string, unknown>) : {}) as Record<string, unknown>;
  const sr = (typeof o.self_review === "object" && o.self_review !== null ? o.self_review : o) as Record<string, unknown>;
  const list = Array.isArray(sr.findings) ? sr.findings : [];
  return list.flatMap((f): Finding[] => {
    const text = typeof f === "string" ? f : typeof f === "object" && f !== null && typeof (f as Record<string, unknown>).text === "string" ? ((f as Record<string, unknown>).text as string) : "";
    if (!text) return [];
    const resolved = typeof f === "object" && f !== null && (f as Record<string, unknown>).resolved === true;
    return [{ kind: "self", label: LABEL.self, text, path: "", open: !resolved }];
  });
}

/** A broken change rule as the Core writes it ("<rule> <class> <path>: <evidence>"), in words: the path and the evidence. */
function rule(s: string): Finding {
  const m = /^\S+\s+\S+\s+(\S*):\s*(.*)$/.exec(s);
  return { kind: "rule", label: LABEL.rule, text: m ? m[2]! || "This change breaks a rule the Core checks." : s, path: m ? m[1]! : "", open: true };
}

/** A check's name in words: a configured command is its command line; another check is its name with the separators spaced. */
export function checkName(id: string): string {
  if (id.startsWith("configured_command:")) return id.slice("configured_command:".length);
  const i = id.indexOf(":");
  return (i >= 0 ? id.slice(i + 1) : id).replace(/[-_]+/g, " ");
}

/** The checks of the Core's latest verification run (the run the review is about). */
const latestChecks = (b: ReviewBundleView) => b.verificationRuns[b.verificationRuns.length - 1]?.checks ?? [];

/** Every finding the review bundle carries, open ones first. A skipped check is not a finding; it is counted in `checksWords`. */
export function findingsOf(b: ReviewBundleView): Finding[] {
  const out: Finding[] = [];
  for (const c of latestChecks(b)) if (c.status !== "PASS" && c.status !== "SKIP") out.push({ kind: "check", label: LABEL.check, text: `${checkName(c.id)} ${c.status === "FAIL" ? "failed" : c.status.toLowerCase().replace(/_/g, " ")}`, path: "", open: true });
  for (const s of b.invariantFindings) out.push(rule(s));
  out.push(...selfReview(b.selfReviewJson));
  for (const c of b.ciEvidence)
    if (c.conclusion === "failure" || c.conclusion === "timed_out" || c.conclusion === "cancelled") out.push({ kind: "forge", label: LABEL.forge, text: `${c.name} ${c.conclusion === "failure" ? "failed" : c.conclusion.replace(/_/g, " ")} on the forge`, path: "", open: true });
  return [...out.filter((f) => f.open), ...out.filter((f) => !f.open)];
}

/** The latest run's checks in words; null when the Core ran none. */
export function checksWords(b: ReviewBundleView): string | null {
  const checks = latestChecks(b);
  if (checks.length === 0) return null;
  const passed = checks.filter((c) => c.status === "PASS").length;
  const skipped = checks.filter((c) => c.status === "SKIP").length;
  const skip = skipped > 0 ? `, ${skipped} skipped` : "";
  if (passed === checks.length) return checks.length === 1 ? "The check passed" : `All ${checks.length} checks passed`;
  if (passed + skipped === checks.length) return `${passed} ${passed === 1 ? "check" : "checks"} passed${skip}`;
  return `${passed} of ${checks.length} checks passed${skip}`;
}
