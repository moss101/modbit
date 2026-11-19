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
export function selfReview(json: string): Finding[] {
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
export function rule(s: string): Finding {
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

/** "1 file", "2 files". */
export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;

/** A verification stage in words. */
export function stageWords(stage: string): string {
  const known: Record<string, string> = { BASELINE: "Before the change", TARGETED: "Checks for this change", COMPLETION: "Final checks", REPAIR: "After a repair" };
  return known[stage] ?? stateWords(stage.charAt(0) + stage.slice(1).toLowerCase());
}

/** A verification run in words: "Before the change: 1 of 1 checks passed". */
export function runWords(v: ReviewBundleView["verificationRuns"][number]): string {
  const passed = v.checks.filter((c) => c.status === "PASS").length;
  const skipped = v.checks.filter((c) => c.status === "SKIP").length;
  return `${stageWords(v.stage)}: ${passed} of ${plural(v.checks.length, "check")} passed${skipped > 0 ? `, ${skipped} skipped` : ""}`;
}

/** A check that did not pass, in words: "Failed: lint", "Skipped: boundary pinned". */
export function checkOutcome(c: { id: string; status: string }): string {
  const word = c.status === "FAIL" ? "Failed" : c.status === "SKIP" ? "Skipped" : stateWords(c.status.charAt(0) + c.status.slice(1).toLowerCase());
  return `${word}: ${checkName(c.id)}`;
}

/** The plan the Core stored, in words; null when it holds neither an outcome nor files. */
export function planWords(json: string): { outcome: string; files: string[] } | null {
  try {
    const v = JSON.parse(json) as Record<string, unknown>;
    const outcome = typeof v.outcome === "string" ? v.outcome : "";
    const files = Array.isArray(v.expected_files) ? v.expected_files.filter((f): f is string => typeof f === "string") : [];
    return outcome || files.length > 0 ? { outcome, files } : null;
  } catch {
    return null;
  }
}

/** The summary the agent gave with its completion, when the stored self-review has one. */
export function selfReviewSummary(json: string): string {
  try {
    const v = JSON.parse(json) as Record<string, unknown>;
    return typeof v.summary === "string" ? v.summary : "";
  } catch {
    return "";
  }
}

const EVIDENCE_KIND: Record<string, string> = { plan: "Plan", tool_result: "Tool result", self_review: "Self-review", report: "Report", receipt: "Effect receipt", verification: "Verification", checkpoint: "Checkpoint" };

/** An evidence link ("kind:hash") as a kind in words and a short hash. */
export function evidenceWords(link: string): { kind: string; short: string } {
  const i = link.indexOf(":");
  const kind = i >= 0 ? link.slice(0, i) : "";
  const ref = i >= 0 ? link.slice(i + 1) : link;
  const words = EVIDENCE_KIND[kind] ?? (kind ? stateWords(kind.replace(/_/g, " ")) : "Record");
  return { kind: words, short: ref.slice(0, 12) };
}

/** A regression attribution ("check=attribution") in words. */
export function attributionWords(a: string): string {
  const i = a.indexOf("=");
  if (i < 0) return a;
  return `${checkName(a.slice(0, i))}: ${a.slice(i + 1).replace(/_/g, " ").toLowerCase()}`;
}

/** A file's status letter in words, capitalised for a file's head. */
export const fileStatusWords = (status: string, binary: boolean): string => (binary ? "Binary" : ({ A: "Added", D: "Deleted", R: "Renamed", M: "Changed" } as Record<string, string>)[status] ?? "Changed");

/** A Core refusal in words, with its code kept for a data attribute: "STALE_REVISION: …" -> the review is out of date. */
export function refusalWords(message: string): { text: string; code: string } {
  const plain = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  const m = /\b([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+)\b:?\s*/.exec(plain);
  const code = m ? m[1]! : "";
  const known: Record<string, string> = {
    STALE_REVISION: "the review is out of date; reload it and try again",
    NO_PULL_REQUEST: "the Core has no pull request for this task, so there is nothing to read yet",
    PROTECTED_PATH: "that file is protected",
  };
  const rest = m ? plain.replace(m[0], "").trim() : plain;
  return { text: known[code] ?? (rest || plain), code };
}
