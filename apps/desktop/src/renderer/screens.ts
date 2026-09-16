/**
 * Screen states (PX-023; docs/39 "Screen flows and states"): every screen
 * has empty, loading, populated, error, degraded and recovery states, and a
 * degraded or error state names its cause, the next action and the
 * evidence reference. Everything here is derived from what the Core said —
 * its status, its events (the reducer's cards), its attention view, its
 * recovery report — never from a timer or a guess; a recovery state shows
 * what the Core recovered and never invents progress.
 */
import type { Diagnostic, TaskCard } from "./model.ts";

export type ScreenName = "fleet" | "newTask" | "task" | "review" | "settings" | "browser";
export type StateKind = "empty" | "loading" | "populated" | "error" | "degraded" | "recovery";

export interface ScreenState {
  screen: ScreenName;
  kind: StateKind;
  /** A short label for the state (`provider down`, `Core restarting`, …). */
  label: string;
  /** Why (present on error, degraded and recovery states). */
  cause?: string;
  /** What to do next (present on error, degraded and recovery states). */
  nextAction?: string;
  /** Where the evidence is (an event offset, an approval id, a gate ref, a receipt). */
  evidence?: string;
}

export interface CoreStatusLike {
  state: "starting" | "connected" | "restarting" | "failed";
  reason?: string;
  restarts: number;
  retryInMs?: number;
}

export interface RecoveryLike {
  bootGeneration: number;
  eventsVerified: number;
  aggregatesVerified: number;
  sessions: number;
  tasks: number;
  projectionsRebuilt: boolean;
  notes: string[];
  recoveryMs: number;
}

export interface AttentionLike {
  kind: string;
  taskId: string;
  reference: string;
  reason: string;
  action: string;
  sinceOffset: string;
}

function state(screen: ScreenName, kind: StateKind, label: string, rest: Partial<ScreenState> = {}): ScreenState {
  return { screen, kind, label, ...rest };
}

/** Home / Fleet. */
export function fleetState(input: { core: CoreStatusLike; loaded: "loading" | "empty" | "populated"; error: string | null; recovery: RecoveryLike | null; recoveredBanner: boolean; tasks: TaskCard[] }): ScreenState {
  const { core } = input;
  if (core.state === "restarting") {
    return state("fleet", "degraded", "Core restarting", {
      cause: `the Core stopped: ${core.reason ?? "unknown reason"}`,
      nextAction: `nothing to do: the last persisted state stays on screen and the Core restarts in ${Math.round((core.retryInMs ?? 0) / 1000)} s (restart ${core.restarts})`,
      evidence: `core status: ${core.reason ?? "restarting"}`,
    });
  }
  if (core.state === "failed") {
    return state("fleet", "error", "Core failed", {
      cause: `the Core could not be kept running: ${core.reason ?? "unknown reason"}`,
      nextAction: "quit and reopen the app; if it repeats, keep the profile directory for a report",
      evidence: `core status after ${core.restarts} restart(s)`,
    });
  }
  if (input.recoveredBanner && input.recovery) {
    const r = input.recovery;
    return state("fleet", "recovery", "Core recovered", {
      cause: `boot ${r.bootGeneration}: the Core verified ${r.eventsVerified} event(s) across ${r.aggregatesVerified} aggregate(s) and recovered ${r.sessions} session(s) and ${r.tasks} task(s) in ${r.recoveryMs} ms${r.projectionsRebuilt ? "; projections were rebuilt from the log" : ""}`,
      nextAction: r.notes.length ? `still to reconcile: ${r.notes.join("; ")}` : "nothing left to reconcile",
      evidence: `recovery report of boot ${r.bootGeneration}`,
    });
  }
  if (input.error) {
    // docs/38 `REGISTRY_STALE`: routing continues on the last-good bundle
    // (the bundle publication itself is M9 substrate, DR-M6-003).
    if (input.error.includes("REGISTRY_STALE")) {
      return state("fleet", "degraded", "stale policy bundle", { cause: input.error, nextAction: "routing continues on the last-good bundle; refresh the registry to route on the current one", evidence: "the command's rejection" });
    }
    return state("fleet", "error", "Core refused", { cause: input.error, nextAction: "read the cause, then retry or fix what it names", evidence: "the command's rejection" });
  }
  if (core.state === "starting" || input.loaded === "loading") return state("fleet", "loading", "loading from the Core");
  // Provider down shows on the tasks it stopped: every waiting task whose
  // diagnostic names the provider.
  const providerDown = input.tasks.find((t) => t.state === "Waiting" && t.diagnostic?.class === "PROVIDER");
  if (providerDown) {
    return state("fleet", "degraded", "provider down", {
      cause: `${providerDown.diagnostic?.code ?? "PROVIDER"}: ${providerDown.diagnostic?.detail ?? "the model provider did not answer"}`,
      nextAction: providerDown.diagnostic?.userAction || "check the provider, then resume the waiting task(s)",
      evidence: `task ${providerDown.taskId.slice(0, 8)} at offset ${providerDown.lastOffset}`,
    });
  }
  if (input.loaded === "empty" || input.tasks.length === 0) return state("fleet", "empty", "no tasks yet");
  return state("fleet", "populated", `${input.tasks.length} task(s)`);
}

/** New Task. */
export function newTaskState(input: { core: CoreStatusLike; provider: { configured: boolean } | null; trusted: string | null; workspaceRoot: string; lastError: string | null }): ScreenState {
  if (input.core.state !== "connected") return state("newTask", "loading", "waiting for the Core");
  if (input.lastError) {
    const e = input.lastError;
    if (e.includes("REPOSITORY_UNTRUSTED") || e.includes("REPOSITORY_MISSING")) {
      return state("newTask", "error", "repository untrusted or missing", { cause: e, nextAction: "point at a Git checkout on this machine and trust it (scoped to that folder)", evidence: "the Core's rejection" });
    }
    if (e.includes("PROFILE_UNSUPPORTED") || e.includes("PLAN_MODE") || e.includes("CAPABILITY_DENIED")) {
      return state("newTask", "error", "policy forbids the requested execution mode", { cause: e, nextAction: "choose a mode the policy allows, or change the policy", evidence: "the Core's rejection" });
    }
    if (e.includes("BUDGET") || e.includes("CAPACITY")) {
      return state("newTask", "degraded", "budget or capacity exhausted", { cause: e, nextAction: "wait for a running task to finish, or raise the budget", evidence: "the Core's rejection" });
    }
    if (e.includes("NO_PROVIDER") || e.includes("PROVIDER")) {
      return state("newTask", "degraded", "provider unavailable", { cause: e, nextAction: "set up a provider (step 2 above) before starting", evidence: "the Core's rejection" });
    }
    return state("newTask", "error", "refused", { cause: e, nextAction: "read the cause, then adjust the request", evidence: "the Core's rejection" });
  }
  if (input.provider && !input.provider.configured) {
    return state("newTask", "degraded", "provider unavailable", { cause: "no provider is configured in this Core", nextAction: "a task can be created but will not start until a provider is set up (step 2 above)", evidence: "provider status" });
  }
  const root = input.workspaceRoot.trim();
  if (root && input.trusted !== root) return state("newTask", "populated", "repository not yet trusted (running trusts it, scoped to it)");
  return state("newTask", "populated", "ready");
}

/** A task's workspace (the card and its attention items). */
export function taskState(input: { card: TaskCard; core: CoreStatusLike; attention: AttentionLike[] }): ScreenState {
  const { card, core } = input;
  if (core.state === "restarting") {
    return state("task", "degraded", "reconnecting by cursor", { cause: `the Core is restarting (${core.reason ?? "unknown reason"})`, nextAction: "nothing to do: the stream resumes from the last cursor when the Core is back", evidence: `cursor ${card.lastOffset}` });
  }
  const d: Diagnostic | null = card.diagnostic;
  const attention = input.attention.filter((a) => a.taskId === card.taskId);
  if (d && (d.class === "UNKNOWN_OUTCOME" || d.code.includes("UNKNOWN_OUTCOME"))) {
    return state("task", "degraded", "unknown tool outcome under reconciliation", { cause: `${d.code}: ${d.detail}`, nextAction: d.userAction || "confirm or deny the effect (ReconcileToolCall), then resume", evidence: d.evidenceRefs.join(", ") || `offset ${card.lastOffset}` });
  }
  if (card.state === "Waiting" && card.waitReason === "Approval" && card.approval) {
    const a = card.approval;
    return state("task", "degraded", "awaiting approval", { cause: `${a.toolName} asks for a ${a.effectClass} effect`, nextAction: `approve or deny intent ${a.intentHash.slice(0, 12)}… (the decision binds exactly this intent)`, evidence: `approval ${a.approvalId}` });
  }
  if (card.feasibility === "QUALITY_FLOOR_INFEASIBLE") {
    return state("task", "degraded", "quality floor infeasible under policy", { cause: "no admitted plan meets the mode's quality floor with the evidence at hand (REQ-EPR-016)", nextAction: "lower the floor, add evidence, or accept the plan as recorded — nothing here claims the target", evidence: `RoutingPlanAdmitted at offset ${card.lastOffset}` });
  }
  if (card.phase === "awaitingHuman" && (card.state === "ReadyForReview" || card.state === "Running" || card.state === "Waiting")) {
    const q = attention.find((a) => a.kind === "QUESTION");
    if (q) return state("task", "degraded", "awaiting your answer", { cause: q.reason, nextAction: q.action, evidence: `question ${q.reference}` });
    return state("task", "degraded", "awaiting human continuation", { cause: card.latestEvidence ?? "the acceptance gate requires a human decision", nextAction: card.nextAction ?? "review and decide", evidence: `offset ${card.lastOffset}` });
  }
  if (card.state === "Failed") {
    return state("task", "error", "failed", { cause: d ? `${d.class}/${d.code}: ${d.detail}` : (card.nextAction ?? "the run failed"), nextAction: d?.userAction || "read the diagnostic; retry if it is retryable", evidence: d?.evidenceRefs.join(", ") || `offset ${card.lastOffset}` });
  }
  if (card.state === "Waiting" && d) {
    return state("task", "degraded", `${d.class.toLowerCase()} stopped the task`, { cause: `${d.code}: ${d.detail}`, nextAction: d.userAction || d.recoveryPath || "resume the task", evidence: d.evidenceRefs.join(", ") || `offset ${card.lastOffset}` });
  }
  const other = attention[0];
  if (other) return state("task", "degraded", other.kind.toLowerCase().replace(/_/g, " "), { cause: other.reason, nextAction: other.action, evidence: other.reference });
  if (card.state === "Completed" || card.state === "Cancelled") return state("task", "populated", card.state.toLowerCase());
  return state("task", "populated", `${card.state.toLowerCase()} · ${card.phase}`);
}

export interface ReviewInputs {
  loading: boolean;
  error: string | null;
  bundle: { taskState: string; workspaceRevision: string; files: unknown[] } | null;
  gate: { verdict: string; missingEvidence: string[]; rejectReasons: string[]; gateRef: string; humanRequired: boolean } | null;
  /** The pull request's last ack: its status, the Core's detail line and
   *  the evidence (an approval for a denial; receipts and the URL for a push). */
  pullRequest: { status: string; detail: string; evidence: string } | null;
}

/** Review. */
export function reviewState(input: ReviewInputs): ScreenState {
  if (input.error) {
    if (input.error.includes("STALE_REVIEW") || input.error.includes("STALE_REVISION")) {
      return state("review", "error", "stale candidate revision", { cause: input.error, nextAction: "reload the review; decide on the revision the Core shows", evidence: "the Core's rejection" });
    }
    return state("review", "error", "refused", { cause: input.error, nextAction: "read the cause; reload the review", evidence: "the Core's rejection" });
  }
  if (input.loading || !input.bundle) return state("review", "loading", "loading the review from the Core");
  if (input.pullRequest && (input.pullRequest.status === "DENIED" || input.pullRequest.status === "FAILED")) {
    return state("review", "degraded", "pull request push denied or failed", { cause: input.pullRequest.detail, nextAction: "decide again to retry the pull request, or leave the branch local", evidence: input.pullRequest.evidence });
  }
  if (input.gate?.verdict === "INCONCLUSIVE") {
    return state("review", "degraded", "verification incomplete", { cause: `acceptance gate INCONCLUSIVE; missing: ${input.gate.missingEvidence.join(", ") || "evidence"}`, nextAction: "run or fix the missing verification before accepting", evidence: `gate ${input.gate.gateRef.slice(0, 12)}` });
  }
  if (input.gate?.verdict === "REJECT") {
    return state("review", "degraded", "acceptance rejected", { cause: input.gate.rejectReasons.join("; ") || "the gate rejected the candidate", nextAction: "return the task to work with what must change", evidence: `gate ${input.gate.gateRef.slice(0, 12)}` });
  }
  if (input.bundle.files.length === 0) return state("review", "empty", "the candidate has no changes");
  return state("review", "populated", `${input.bundle.files.length} file(s) at revision ${input.bundle.workspaceRevision}`);
}

/** Browser (M7.1, docs/39 "Browser" row: the takeover and visual-fallback
 *  states arrive with M7.6 and M7.5; DR-M6-003). */
export function browserState(input: { opening: boolean; error: string | null; host: { attached: boolean; shown: boolean; url: string; title: string; stateVersion: number } | null; gone: string | null; controller: string }): ScreenState {
  if (input.error) {
    return state("browser", "error", "browser session refused", { cause: input.error, nextAction: "read the cause; open the session again", evidence: "the Core's or the host's rejection" });
  }
  if (input.gone) {
    return state("browser", "degraded", "session lost", { cause: `the page's renderer process is gone: ${input.gone}`, nextAction: "reopen the session (the same partition keeps its cookies and storage) or close it", evidence: `host report: ${input.gone}` });
  }
  if (input.opening || !input.host) return state("browser", "loading", "opening the session");
  if (!input.host.attached) {
    return state("browser", "degraded", "host not attached", { cause: "the view is not attached to the Core's session (the Core restarted or refused the attach)", nextAction: "the host attaches again when the Core is back; otherwise reopen", evidence: `state version ${input.host.stateVersion}` });
  }
  if (input.controller === "USER") {
    return state("browser", "degraded", "takeover active", { cause: "you hold control of the session; agent input is blocked, observation is not", nextAction: "return control to let the agent act again", evidence: "control lease" });
  }
  if (!input.host.url || input.host.url === "about:blank") return state("browser", "empty", "no page yet");
  return state("browser", "populated", `${input.host.title || input.host.url} · state ${input.host.stateVersion}`);
}

/** Settings (credential custody). */
export function settingsState(input: { provider: { keychainAvailable: boolean; configured: boolean } | null; organizationOverrides: { key: string; value: string; by: string }[] }): ScreenState {
  if (!input.provider) return state("settings", "loading", "loading provider status");
  if (!input.provider.keychainAvailable) {
    return state("settings", "degraded", "keychain unavailable", { cause: "the OS keychain (safeStorage) is not available; a provider key is held for this session only", nextAction: "unlock or enable the OS keychain, then enter the key again to keep it", evidence: "provider status: keychainAvailable=false" });
  }
  if (input.organizationOverrides.length) {
    return state("settings", "populated", `${input.organizationOverrides.length} organization override(s) shown`);
  }
  return state("settings", "populated", "ready");
}
