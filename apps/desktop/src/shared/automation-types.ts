/**
 * The plain, structured-clone-safe shapes the automations surface crosses the
 * preload bridge as (REQ-PX-086; docs/68). Main converts the Core's protobuf
 * messages to these (times as epoch milliseconds, counts as numbers: none of
 * them can exceed 2^53 in this surface); the renderer renders them and decides
 * nothing: whether a definition is valid, what its hash is, what an approval
 * must list and what a run became are the Core's answers.
 */

export interface AutoIssue {
  path: string;
  code: string;
  message: string;
}

export interface AutoTrigger {
  id: string;
  kind: string;
  summary: string;
  /** Epoch ms of the next due slot of a schedule trigger; 0 when none. */
  nextDueMs: number;
}

export interface AutoEnable {
  version: number;
  definitionHash: string;
  effects: string;
  capabilities: string[];
  paths: string[];
  hosts: string[];
  approver: string;
  approvedMs: number;
  repoRevision: string;
  sourceSha256: string;
}

/** The Core's definition states. */
export const AUTO_STATES = ["ENABLED", "PAUSED", "DISABLED", "NEEDS_APPROVAL", "AUTO_DISABLED"] as const;
export type AutoState = (typeof AUTO_STATES)[number];

export interface AutoView {
  automationId: string;
  name: string;
  description: string;
  currentVersion: number;
  definitionHash: string;
  state: string;
  disabledReason: string;
  disabledDetail: string;
  paused: boolean;
  enabled: AutoEnable | null;
  sourceKind: string;
  sourcePath: string;
  workspaceRoot: string;
  principal: string;
  triggers: AutoTrigger[];
  effects: string;
  capabilities: string[];
  paths: string[];
  hosts: string[];
  consecutiveFailures: number;
  lastRunMs: number;
  lastRunStatus: string;
  definitionJson: string;
  needsListedApproval: boolean;
  contentChanged: boolean;
  queued: number;
  active: number;
  limitDeadlineMinutes: number;
  limitMaxCostMinor: number;
  limitApprovalWaitMinutes: number;
}

export interface AutoVersion {
  version: number;
  definitionHash: string;
  definitionJson: string;
  sourceKind: string;
  sourcePath: string;
  sourceRevision: string;
  createdMs: number;
  createdBy: string;
  approved: boolean;
}

export interface AutoDetail {
  view: AutoView | null;
  versions: AutoVersion[];
}

export interface AutoRun {
  dispatchKey: string;
  automationId: string;
  name: string;
  version: number;
  triggerId: string;
  triggerKind: string;
  eventId: string;
  status: string;
  reason: string;
  detail: string;
  /** Hex, or "" when no task was created. */
  taskId: string;
  sessionId: string;
  principal: string;
  firedMs: number;
  dispatchedMs: number;
  finishedMs: number;
  costMinor: number;
  test: boolean;
  catchUp: boolean;
  missed: number;
  findings: number;
  outputsJson: string;
  acknowledged: boolean;
  slotMs: number;
}

export interface AutoAttention {
  kind: string;
  automationId: string;
  name: string;
  dispatchKey: string;
  taskId: string;
  reason: string;
  action: string;
  atMs: number;
}

export interface AutoList {
  automations: AutoView[];
  globalPaused: boolean;
  attention: AutoAttention[];
  nowMs: number;
  clock: string;
}

export interface AutoValidation {
  ok: boolean;
  issues: AutoIssue[];
  name: string;
  definitionHash: string;
  canonicalJson: string;
  effects: string;
  capabilities: string[];
  paths: string[];
  hosts: string[];
  needsListedApproval: boolean;
  triggers: AutoTrigger[];
}

export interface AutoRepoLoad {
  loaded: AutoView[];
  problems: AutoIssue[];
}

export interface AutoRunStarted {
  dispatchKey: string;
  status: string;
  reason: string;
  detail: string;
  taskId: string;
  wouldSkipByFilter: boolean;
}

export interface AutoKillReport {
  cancelledRuns: number;
  droppedQueued: number;
  paused: boolean;
}

/** What the person approved: exactly the values on the dialog, nothing derived in main. */
export interface AutoEnableInput {
  automationId: string;
  version: number;
  definitionHash: string;
  effects: string;
  capabilities: string[];
  paths: string[];
  hosts: string[];
}

export interface AutoRunInput {
  automationId: string;
  triggerId?: string;
  inputsJson?: string;
  test?: boolean;
  payloadJson?: string;
  source?: string;
  event?: string;
  eventId?: string;
}
