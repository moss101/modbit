/**
 * The automations surface's pure view model (REQ-PX-086; docs/68). It turns
 * what the Core said into words, tones, the exact approval the dialog sends
 * and the actions an attention item offers. It decides nothing: validity, the
 * hash, what an approval must list, the next slot and the state of a
 * definition are the Core's. In particular nothing here parses or validates a
 * definition (QUAL-PX-086: a renderer that validates locally and disagrees with
 * the Core fails the single-source check), and the approval is built only from
 * the values the Core reported for the exact version the person is looking at.
 */
import type { AutoAttention, AutoEnableInput, AutoRun, AutoTrigger, AutoView } from "../../shared/automation-types.ts";

export type Tone = "ok" | "warn" | "danger" | "info" | "idle" | "running";

export const STATE_META: Record<string, { label: string; tone: Tone; words: string }> = {
  ENABLED: { label: "Enabled", tone: "ok", words: "Runs when a trigger fires." },
  PAUSED: { label: "Paused", tone: "warn", words: "Enabled, but nothing starts while it is paused." },
  DISABLED: { label: "Disabled", tone: "idle", words: "Not approved, or turned off by its owner." },
  NEEDS_APPROVAL: { label: "Needs approval", tone: "warn", words: "This version has not been approved, so no trigger can start it." },
  AUTO_DISABLED: { label: "Auto-disabled", tone: "danger", words: "Turned itself off after repeated failures." },
};

export function stateMeta(state: string): { label: string; tone: Tone; words: string } {
  return STATE_META[state] ?? { label: state || "Unknown", tone: "idle", words: "A state this app does not know; the Core's word is shown as it is." };
}

/** Run statuses, with the tone each is shown in. */
export const RUN_STATUS_META: Record<string, { label: string; tone: Tone }> = {
  pending: { label: "Pending", tone: "info" },
  queued: { label: "Queued", tone: "info" },
  running: { label: "Running", tone: "running" },
  succeeded: { label: "Succeeded", tone: "ok" },
  failed: { label: "Failed", tone: "danger" },
  skipped: { label: "Skipped", tone: "idle" },
  cancelled: { label: "Cancelled", tone: "warn" },
};

export function runStatusMeta(status: string): { label: string; tone: Tone } {
  return RUN_STATUS_META[status] ?? { label: status || "Unknown", tone: "idle" };
}

/** The Core's typed reasons for a skip, a failure or a cancellation, in words. An unknown reason is shown as the Core sent it. */
export const REASON_WORDS: Record<string, string> = {
  DUPLICATE: "A delivery with the same event id had already been seen, so nothing was created.",
  PAUSED: "The definition or the global switch was paused when this fired.",
  BUDGET: "The hourly run limit or the daily budget was used up.",
  BUDGET_EXHAUSTED: "The run's cost limit was reached and it was stopped.",
  GATE: "The gate step said this run was not needed.",
  CONCURRENCY: "Another run of this definition was still active and its policy skips overlap.",
  MISSED: "A slot that passed while the Core was not running.",
  APPROVAL_EXPIRED: "A protected effect waited for an approval nobody gave before it expired.",
  POLICY: "The principal's policy did not allow the run.",
  FILTER: "The trigger's filters did not match this event.",
  KILLED: "Stopped by a kill switch.",
  REPLACED: "A newer run replaced this one at a safe boundary.",
  DEADLINE: "The run's time limit passed.",
  TASK_COMPLETED: "The task completed; its result is with you in the run's worktree.",
  TASK_FAILED: "The task the run created failed.",
  TASK_STOPPED: "The task was stopped.",
  TASK_ENDED: "The task ended without completing.",
  CORE_RESTARTED: "The Core restarted while this was in flight.",
  NO_PROGRESS: "The run stopped making progress.",
  SOURCE_CHANGED: "The repository file no longer matches the bytes that were approved.",
  NEEDS_APPROVAL: "The version has no enable approval.",
  PROVIDER_FAILED: "The model provider failed.",
  EMERGENCY_STOP: "An emergency stop cancelled the run.",
};

export function reasonWords(reason: string): string {
  if (!reason) return "";
  return REASON_WORDS[reason] ?? reason;
}

/** The refusal codes of the automation commands the dialogs name in words. */
export const REFUSAL_WORDS: Record<string, string> = {
  APPROVAL_MISMATCH: "The definition changed since you opened this: the Core only accepts an approval of the current version and hash. Review the current version and approve that.",
  APPROVAL_LISTS_DIFFER: "The lists in the approval do not match what the definition asks for. Nothing was enabled.",
  REPOSITORY_UNTRUSTED: "The workspace has not been trusted. An automation acts only in a workspace you have trusted.",
  INVALID_DEFINITION: "The Core did not accept the definition. Nothing was saved.",
  NOT_ENABLED: "No approval names the current version, so it cannot run. Approve it first.",
  NEEDS_APPROVAL: "A definition from a repository file is data until you approve it.",
  SOURCE_CHANGED: "The repository file changed since it was loaded. Reload it from the repository and review it again.",
  READ_ONLY_SOURCE: "A definition supplied by a repository file is changed in the file, then loaded again.",
  INPUT_NEEDS_DEFAULT: "A trigger that starts without a person needs a default for every required input.",
  NOT_MANUAL: "A run by hand uses a manual trigger.",
  NO_MANUAL_TRIGGER: "The definition has no manual trigger.",
  UNKNOWN_AUTOMATION: "The Core does not know this definition any more.",
  REPOSITORY_MISSING: "That folder does not exist on this machine.",
  BAD_ARGUMENT: "The request was not well formed.",
};

/** Splits a main-process error (`Error invoking remote method 'x': Error: CODE: message`) into the Core's code and message. */
export function refusalOf(message: string): { code: string; message: string } {
  const text = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  const m = /^([A-Z][A-Z0-9_]{2,}): ?([\s\S]*)$/.exec(text);
  return m ? { code: m[1]!, message: m[2]! } : { code: "", message: text };
}

export function refusalSentence(message: string): string {
  const r = refusalOf(message);
  const words = REFUSAL_WORDS[r.code];
  return words ? `${words}${r.message ? ` (${r.code}: ${r.message})` : ` (${r.code})`}` : r.code ? `${r.code}: ${r.message}` : r.message;
}

/** Whether a version can be put in front of the person for approval: it is not already enabled, and a changed repository file must be loaded again first. */
export function approvalBlocked(v: AutoView): string | null {
  if (v.state === "ENABLED" || v.state === "PAUSED") return "This version is already approved.";
  if (v.sourceKind === "repository" && v.contentChanged) return "The repository file changed on disk since it was loaded. Reload it from the repository, then review the new version.";
  return null;
}

/**
 * The approval exactly as the dialog shows it: the version, the full hash and
 * the lists the Core reported for the version the person is looking at. These
 * are the values sent, unchanged; nothing is recomputed here.
 */
export function enableApprovalOf(v: AutoView): AutoEnableInput {
  return { automationId: v.automationId, version: v.currentVersion, definitionHash: v.definitionHash, effects: v.effects, capabilities: [...v.capabilities], paths: [...v.paths], hosts: [...v.hosts] };
}

export const EFFECT_WORDS: Record<string, string> = {
  read_only: "Reads only. Nothing can be changed.",
  reversible_write: "May write inside its own worktree.",
  protected_write: "May ask to write to protected paths; each effect still asks.",
  external_side_effect: "May ask for effects beyond this machine; each effect still asks.",
};

export function effectWords(effects: string): string {
  return EFFECT_WORDS[effects] ?? effects;
}

/** A moment in UTC, to the minute. */
export function utcMinute(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getUTCFullYear()}-${p(d.getUTCMonth() + 1)}-${p(d.getUTCDate())} ${p(d.getUTCHours())}:${p(d.getUTCMinutes())} UTC`;
}

/** "in 4 min" / "2 h ago" against the Core's own clock reading. */
export function relativeTo(ms: number, nowMs: number): string {
  if (!ms) return "";
  const diff = ms - nowMs;
  const abs = Math.abs(diff);
  const unit = abs < 90_000 ? `${Math.max(1, Math.round(abs / 1000))} s` : abs < 5_400_000 ? `${Math.round(abs / 60_000)} min` : abs < 129_600_000 ? `${Math.round(abs / 3_600_000)} h` : `${Math.round(abs / 86_400_000)} d`;
  return diff >= 0 ? `in ${unit}` : `${unit} ago`;
}

export function nextDueWords(t: AutoTrigger, nowMs: number): string {
  if (t.kind !== "schedule") return "";
  return t.nextDueMs ? `next ${utcMinute(t.nextDueMs)} (${relativeTo(t.nextDueMs, nowMs)})` : "no slot due";
}

/** The next slot of any schedule trigger of a definition, or 0. */
export function nextDueOf(v: AutoView): number {
  const due = v.triggers.filter((t) => t.kind === "schedule" && t.nextDueMs > 0).map((t) => t.nextDueMs);
  return due.length ? Math.min(...due) : 0;
}

/** What a test run reports as the protected effects it denied and listed as what would have been asked. */
export interface WouldHaveAsked {
  dryRun: boolean;
  taskState: string;
  entries: { tool: string; outcome: string; decision: string }[];
}

export function wouldHaveAskedOf(outputsJson: string): WouldHaveAsked | null {
  if (!outputsJson) return null;
  let o: unknown;
  try {
    o = JSON.parse(outputsJson);
  } catch {
    return null;
  }
  if (typeof o !== "object" || o === null) return null;
  const r = o as { dry_run?: unknown; task_state?: unknown; would_have_asked?: unknown };
  const list = Array.isArray(r.would_have_asked) ? r.would_have_asked : [];
  const entries = list
    .filter((e): e is { tool?: unknown; outcome?: unknown; decision?: unknown } => typeof e === "object" && e !== null)
    .map((e) => ({ tool: typeof e.tool === "string" ? e.tool : "", outcome: typeof e.outcome === "string" ? e.outcome : "", decision: typeof e.decision === "string" ? e.decision : e.decision === undefined || e.decision === null ? "" : JSON.stringify(e.decision) }));
  return { dryRun: r.dry_run === true, taskState: typeof r.task_state === "string" ? r.task_state : "", entries };
}

/** Run duration in words; "" while it has not finished. */
export function durationWords(r: AutoRun): string {
  if (!r.finishedMs || !r.dispatchedMs) return "";
  const s = Math.max(0, Math.round((r.finishedMs - r.dispatchedMs) / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** Cost in minor units, as the Core counted it. */
export function costWords(minor: number): string {
  return minor > 0 ? `${minor} minor units` : "none counted";
}

export const ATTENTION_META: Record<string, { label: string; tone: Tone }> = {
  RUN_FAILED: { label: "A run failed", tone: "danger" },
  PARKED_APPROVAL: { label: "A run is waiting for your approval", tone: "warn" },
  APPROVAL_EXPIRED: { label: "An approval expired and the run was cancelled", tone: "warn" },
  AUTO_DISABLED: { label: "Disabled itself after repeated failures", tone: "danger" },
  SOURCE_CHANGED: { label: "The repository file changed", tone: "warn" },
  SKIPPED_BUDGET: { label: "A run was skipped: budget used up", tone: "warn" },
};

export type AttentionAction = { kind: "acknowledge"; label: string } | { kind: "review"; label: string } | { kind: "approval"; label: string };

/** What clears an attention item: the Core names the command; this names the control. */
export function attentionAction(a: AutoAttention): AttentionAction {
  if (a.action === "EnableAutomation") return { kind: "review", label: a.kind === "SOURCE_CHANGED" ? "Review and approve again" : "Review and enable" };
  if (a.action === "ResolveApproval") return { kind: "approval", label: "Open the approval" };
  return { kind: "acknowledge", label: "Acknowledge" };
}

/** A starter the editor can insert: a read-only manual definition (data the person edits; nothing about it is validated here). */
export const STARTER_DEFINITION = JSON.stringify(
  {
    schema: "modbit.automation/1",
    name: "drift-report",
    description: "Report whether the default branch has moved ahead of the local one.",
    prompt: "Report whether the default branch has moved ahead of the local one. Do not change anything.",
    triggers: [{ kind: "manual", id: "now" }],
  },
  null,
  2,
);

/** Test-run payload starters for the trigger kinds that take one. */
export function samplePayloadFor(kind: string): string {
  // The normalized event the Core's filters read: branch, labels, author, paths, title and text.
  if (kind === "event") return JSON.stringify({ action: "opened", branch: "main", labels: ["needs-review"], author: "someone", paths: ["src/lib.rs"], title: "Sample pull request", text: "" }, null, 2);
  if (kind === "webhook") return JSON.stringify({ ref: "refs/heads/main" }, null, 2);
  return "";
}

/** Whether a trigger can be run by hand, tested with a payload, or only fires on its own. */
export function runKinds(v: AutoView): { manual: AutoTrigger | null; payload: AutoTrigger[]; schedule: AutoTrigger[] } {
  return {
    manual: v.triggers.find((t) => t.kind === "manual") ?? null,
    payload: v.triggers.filter((t) => t.kind === "event" || t.kind === "webhook"),
    schedule: v.triggers.filter((t) => t.kind === "schedule"),
  };
}

/** Rows of a run's one-line summary for the history table. */
export function runSummary(r: AutoRun): string {
  const parts = [r.triggerKind || "trigger"];
  if (r.test) parts.push("test");
  if (r.catchUp) parts.push(`catch-up (${r.missed} missed)`);
  return parts.join(" · ");
}
