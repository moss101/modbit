/**
 * Fixtures for the Automations surface in the model-free state gallery
 * (REQ-PX-086). Every value is invented for design review: no Core, no model
 * and no run is behind it, and a gallery state is never evidence that a
 * behaviour works (automations.spec.ts proves those against a real Core).
 */
import type { AutoAttention, AutoDetail, AutoList, AutoRun, AutoValidation, AutoView } from "../../shared/automation-types.ts";
import type { AutomationCalls } from "../automations/calls.ts";
import { GALLERY_NOW } from "./workspace-fixtures.ts";

const H = (c: string) => c.repeat(64);
const ID = (c: string) => c.repeat(32);

const base = (id: string, name: string, over: Partial<AutoView>): AutoView => ({
  automationId: id, name, description: "", currentVersion: 1, definitionHash: H("a"), state: "ENABLED", disabledReason: "", disabledDetail: "", paused: false, enabled: null, sourceKind: "local", sourcePath: "",
  workspaceRoot: "/work/example", principal: "creator", triggers: [{ id: "now", kind: "manual", summary: "manual / test run", nextDueMs: 0 }], effects: "read_only", capabilities: [], paths: [], hosts: [],
  consecutiveFailures: 0, lastRunMs: GALLERY_NOW - 3_600_000, lastRunStatus: "succeeded", definitionJson: "{}", needsListedApproval: false, contentChanged: false, queued: 0, active: 0, limitDeadlineMinutes: 30, limitMaxCostMinor: 0, limitApprovalWaitMinutes: 1440, ...over,
});

const enabled = (hash: string, over: Partial<NonNullable<AutoView["enabled"]>> = {}): NonNullable<AutoView["enabled"]> => ({ version: 1, definitionHash: hash, effects: "read_only", capabilities: [], paths: [], hosts: [], approver: "user:fixture", approvedMs: GALLERY_NOW - 86_400_000, repoRevision: "", sourceSha256: "", ...over });

const DEF_JSON = JSON.stringify({ schema: "modbit.automation/1", name: "drift-report", prompt: "Report whether the default branch has moved ahead.", triggers: [{ kind: "manual", id: "now" }] }, null, 2);

export const WRITER: AutoView = base(ID("2"), "report-writer", {
  state: "NEEDS_APPROVAL", description: "Writes a weekly report under reports/", definitionHash: H("c"), currentVersion: 3, effects: "reversible_write", capabilities: ["fs.write"], paths: ["reports/**"], needsListedApproval: true,
  triggers: [{ id: "weekly", kind: "schedule", summary: "cron 0 6 * * 1 (UTC)", nextDueMs: GALLERY_NOW + 3 * 86_400_000 }], lastRunMs: 0, lastRunStatus: "",
  disabledReason: "EDITED", disabledDetail: "version 3 needs its own approval",
});
export const REPO_DEF: AutoView = base(ID("7"), "nightly-check", {
  state: "NEEDS_APPROVAL", sourceKind: "repository", sourcePath: ".modbit/automations/nightly.json", definitionHash: H("d"), lastRunMs: 0, lastRunStatus: "",
  triggers: [{ id: "pr", kind: "event", summary: "forge pull_request (opened)", nextDueMs: 0 }],
});
export const REPO_CHANGED: AutoView = { ...REPO_DEF, automationId: ID("8"), name: "weekly-audit", state: "ENABLED", contentChanged: true, definitionHash: H("e"), enabled: enabled(H("e"), { repoRevision: "1f3c9a07b2d44e55a1c0", sourceSha256: H("9") }) };

export const ALL_STATES: AutoView[] = [
  base(ID("1"), "drift-report", { state: "ENABLED", definitionHash: H("a"), enabled: enabled(H("a")), definitionJson: DEF_JSON, triggers: [{ id: "tick", kind: "schedule", summary: "every 60 min", nextDueMs: GALLERY_NOW + 1_800_000 }, { id: "now", kind: "manual", summary: "manual / test run", nextDueMs: 0 }] }),
  base(ID("3"), "dependency-audit", { state: "PAUSED", paused: true, definitionHash: H("b"), enabled: enabled(H("b")), lastRunStatus: "skipped", triggers: [{ id: "daily", kind: "schedule", summary: "cron 0 3 * * * (UTC)", nextDueMs: GALLERY_NOW + 15 * 3_600_000 }] }),
  base(ID("4"), "old-experiment", { state: "DISABLED", disabledReason: "OWNER", lastRunMs: 0, lastRunStatus: "" }),
  WRITER,
  base(ID("5"), "flaky-watcher", { state: "AUTO_DISABLED", disabledReason: "CONSECUTIVE_FAILURES", disabledDetail: "5 consecutive failures", consecutiveFailures: 5, lastRunStatus: "failed", triggers: [{ id: "hook", kind: "webhook", summary: "signed webhook `ci`", nextDueMs: 0 }] }),
  REPO_DEF,
  REPO_CHANGED,
];

const att = (kind: string, view: AutoView, action: string, reason: string, over: Partial<AutoAttention> = {}): AutoAttention => ({ kind, automationId: view.automationId, name: view.name, dispatchKey: `${kind.toLowerCase()}-key`, taskId: "", reason, action, atMs: GALLERY_NOW - 60_000, ...over });

export const ATTENTION: AutoAttention[] = [
  att("RUN_FAILED", ALL_STATES[0]!, "AckAutomationAttention", "TASK_FAILED: the task the run created failed", { taskId: ID("e") }),
  att("PARKED_APPROVAL", ALL_STATES[0]!, "ResolveApproval", "shell.exec asks for a ProtectedWrite effect; it expires if nobody answers", { taskId: ID("f") }),
  att("APPROVAL_EXPIRED", ALL_STATES[1]!, "AckAutomationAttention", "APPROVAL_EXPIRED: waited 1440 min"),
  att("AUTO_DISABLED", ALL_STATES[4]!, "EnableAutomation", "5 consecutive failures"),
  att("SOURCE_CHANGED", REPO_CHANGED, "EnableAutomation", "the file no longer has the approved bytes"),
  att("SKIPPED_BUDGET", ALL_STATES[1]!, "AckAutomationAttention", "BUDGET: the hourly limit was used up"),
];

const LIST: AutoList = { automations: ALL_STATES, globalPaused: false, attention: ATTENTION, nowMs: GALLERY_NOW, clock: "WALL" };
export const EMPTY_LIST: AutoList = { automations: [], globalPaused: false, attention: [], nowMs: GALLERY_NOW, clock: "WALL" };

const run = (n: number, over: Partial<AutoRun>): AutoRun => ({
  dispatchKey: `${String(n).padStart(2, "0")}`.repeat(32), automationId: ID("1"), name: "drift-report", version: 1, triggerId: "tick", triggerKind: "schedule", eventId: `tick@${GALLERY_NOW - n * 3_600_000}`, status: "succeeded", reason: "TASK_COMPLETED", detail: "", taskId: ID("e"), sessionId: ID("d"),
  principal: "creator", firedMs: GALLERY_NOW - n * 3_600_000, dispatchedMs: GALLERY_NOW - n * 3_600_000, finishedMs: GALLERY_NOW - n * 3_600_000 + 42_000, costMinor: 12, test: false, catchUp: false, missed: 0, findings: 0, outputsJson: "", acknowledged: false, slotMs: 0, ...over,
});

/** One run for every typed reason the Core records, plus a test run, a catch-up and a payload with findings. */
export const HISTORY: AutoRun[] = [
  run(1, {}),
  run(2, { status: "failed", reason: "TASK_FAILED", detail: "verification failed", taskId: ID("e") }),
  run(3, { status: "skipped", reason: "DUPLICATE", taskId: "", costMinor: 0, triggerKind: "event", detail: "a delivery with this id was already seen" }),
  run(4, { status: "skipped", reason: "PAUSED", taskId: "", costMinor: 0 }),
  run(5, { status: "skipped", reason: "BUDGET", taskId: "", costMinor: 0, detail: "hourly limit" }),
  run(6, { status: "skipped", reason: "CONCURRENCY", taskId: "", costMinor: 0 }),
  run(7, { status: "skipped", reason: "MISSED", taskId: "", costMinor: 0, detail: "the Core was not running" }),
  run(8, { status: "cancelled", reason: "APPROVAL_EXPIRED", detail: "waited 1440 min" }),
  run(9, { status: "cancelled", reason: "KILLED", detail: "stopped by the kill switch" }),
  run(10, { status: "cancelled", reason: "REPLACED", detail: "a newer run replaced this one" }),
  run(11, { status: "failed", reason: "BUDGET_EXHAUSTED", detail: "the run's cost limit was reached" }),
  run(12, { status: "skipped", reason: "FILTER", taskId: "", costMinor: 0, triggerKind: "event" }),
  run(13, { catchUp: true, missed: 6, reason: "TASK_COMPLETED", detail: "one run for the whole window" }),
  run(14, { test: true, triggerKind: "event", findings: 2, outputsJson: JSON.stringify({ dry_run: true, task_state: "ReadyForReview", would_have_asked: [{ tool: "change.apply", outcome: "WOULD_HAVE_ASKED", decision: "Ask" }, { tool: "net.fetch", outcome: "DENIED", decision: "Deny" }] }) }),
  run(15, { status: "running", reason: "", finishedMs: 0, costMinor: 0 }),
];

function detailOf(view: AutoView): AutoDetail {
  return {
    view,
    versions: [{ version: view.currentVersion, definitionHash: view.definitionHash, definitionJson: view.definitionJson, sourceKind: view.sourceKind, sourcePath: view.sourcePath, sourceRevision: view.sourceKind === "repository" ? "1f3c9a07b2d44e55a1c0" : "", createdMs: GALLERY_NOW - 86_400_000, createdBy: "user:fixture", approved: view.state === "ENABLED" }],
  };
}

const none = (what: string) => () => Promise.reject(new Error(`FIXTURE: ${what} has no Core behind it`));

/** The verdict a fixture editor shows for a document (the real one is the Core's). */
export const FIXTURE_ISSUES: AutoValidation = {
  ok: false,
  issues: [
    { path: "/triggers/0/cron", code: "BELOW_FLOOR", message: "a schedule runs at most once every 5 minutes; this one runs every 2" },
    { path: "/triggers/1/filters/title_regex", code: "BAD_REGEX", message: "unclosed group" },
    { path: "/profile/capabilities/0", code: "UNKNOWN_CAPABILITY", message: "computer.control is never available to an automation" },
  ],
  name: "", definitionHash: "", canonicalJson: "", effects: "", capabilities: [], paths: [], hosts: [], needsListedApproval: false, triggers: [],
};

export const BAD_DEFINITION = JSON.stringify({ schema: "modbit.automation/1", name: "bad", prompt: "p", triggers: [{ kind: "schedule", id: "fast", cron: "*/2 * * * *" }, { kind: "event", id: "pr", source: "forge", event: "pull_request", filters: { title_regex: "(unclosed" } }], profile: { capabilities: ["computer.control"] } }, null, 2);

export function fixtureCalls(list: AutoList, runs: AutoRun[]): AutomationCalls {
  return {
    list: () => Promise.resolve(list),
    get: (id) => {
      const v = list.automations.find((x) => x.automationId === id);
      return v ? Promise.resolve(detailOf(v)) : Promise.reject(new Error("UNKNOWN_AUTOMATION: fixture"));
    },
    runs: (o) => Promise.resolve(runs.filter((r) => !o.automationId || r.automationId === o.automationId)),
    validate: () => Promise.resolve(FIXTURE_ISSUES),
    create: none("save"),
    update: none("save"),
    loadRepository: none("reload"),
    enable: none("approve"),
    disable: none("disable"),
    pause: none("pause"),
    kill: () => Promise.resolve({ cancelledRuns: 1, droppedQueued: 2, paused: true }),
    run: none("run"),
    ack: none("acknowledge"),
  };
}

export const FIXTURE_LIST = LIST;
