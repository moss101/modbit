/**
 * The plain, structured-clone-safe shapes the approval stack, the run modes,
 * the context ring and the checkpoint surface cross the preload bridge as
 * (REQ-PX-057, 058, 060, 062). Main converts the Core's protobuf messages to
 * these (64-bit counts as decimal strings, ids as hex or the Core's own text,
 * enums as their stable names); the renderer renders them and decides
 * nothing: which mode approves what, whether a rule is valid, what a restore
 * changes and whether a redo is still exact are the Core's answers.
 */

/** The run modes, least to most approving (the Core's own list; the view model reads `RunModeView.modes`). */
export const RUN_MODES = ["ASK", "ALLOWLIST", "ALLOWLIST_SANDBOX", "RUN_EVERYTHING"] as const;
export type RunModeName = (typeof RUN_MODES)[number];

/** Why the Core asked, from the typed `ask_reason` of the approval's scope (AFW-F12). */
export interface ApprovalReasonView {
  /** `MODE_ASK` | `NOT_IN_ALLOWLIST` | `ALWAYS_ASK:<CLASS>` | "" when the Core gave none (the card then says so). */
  code: string;
  runMode: string;
  /** The always-ask classes the call falls in. */
  askClasses: string[];
  /** The call runs fully contained in a sandbox (the host says so). */
  contained: boolean;
  capabilities: string[];
  executionProfile: string;
}

/** The exact intent of a pending effect, read from the call's recorded arguments (bounded; the hash is the Core's). */
export interface ApprovalIntentView {
  /** The arguments were found; false = only the tool, the class and the hash are known. */
  found: boolean;
  argv: string[] | null;
  /** A shell command line, when the call carried one instead of an argv. */
  command: string;
  cwd: string;
  /** Paths the call names (read or written). */
  paths: string[];
  /** Hosts the call would contact. */
  hosts: string[];
  /** The declared escalation (none | network | all), "" when the call declared none. */
  escalation: string;
  /** The arguments as compact JSON, long strings clipped; the whole of what the model asked. */
  argsPreview: string;
  truncated: boolean;
}

/** A protected effect waiting for the person's decision, with everything the card shows. */
export interface DockApprovalView {
  approvalId: string;
  taskId: string;
  toolCallId: string;
  toolName: string;
  effectClass: string;
  /** The intent hash a decision must name; the Core refuses any other. */
  intentHash: string;
  requestedAtMs: number;
  /** 0 = never. */
  expiresAtMs: number;
  reason: ApprovalReasonView;
  intent: ApprovalIntentView;
}

/** An approval the Core closed as expired: nobody answered before its recorded expiry, so it can no longer be approved and the effect did not run (REQ-PX-058). */
export interface ExpiredApprovalView {
  approvalId: string;
  taskId: string;
  toolName: string;
  effectClass: string;
  intentHash: string;
  /** The expiry the Core recorded, epoch ms. */
  expiresAtMs: number;
}

export const PROPOSAL_STATUSES = ["PENDING", "ACCEPTED", "DECLINED", "SKIPPED"] as const;
export type ProposalStatus = (typeof PROPOSAL_STATUSES)[number];

/** The agent's request to move the task to another mode (REQ-PX-055). It is data until the person accepts it; the words in `reason` are the agent's and are untrusted. */
export interface ModeProposalInfo {
  proposalId: string;
  taskId: string;
  fromMode: string;
  toMode: string;
  reason: string;
  status: ProposalStatus;
  /** Why it ended: ACCEPTED_BY_USER | DECLINED_BY_USER | UNANSWERED_15S | CORE_RESTARTED | TASK_ENDED | MODE_CHANGED; empty while pending. */
  outcomeReason: string;
  proposedAtMs: number;
  expiresAtMs: number;
  decidedAtMs: number;
}

export interface ModeProposalListInfo {
  taskId: string;
  proposals: ModeProposalInfo[];
  /** The Core's clock when it answered (epoch ms): the time left is read against it, not against this machine's. */
  nowMs: number;
}

export interface ModeProposalDecisionInfo {
  proposal: ModeProposalInfo;
  /** On accept, the mode the Core moved the task to and when it applies. */
  modeChange: { mode: string; previousMode: string; effective: string } | null;
}

export interface RunModeInfo {
  taskId: string;
  mode: string;
  acknowledged: boolean;
  alwaysAsk: string[];
  modes: string[];
  warning: string;
  sessionOnly: boolean;
  offset: string;
  rulesInForce: number;
}

export interface AllowRuleInfo {
  ruleId: string;
  pattern: string[];
  scope: string;
  scopeKey: string;
  createdBy: string;
  createdAtMs: number;
  /** 0 = none. */
  expiresAtMs: number;
  coversAlwaysAsk: boolean;
  state: string;
  revokedBy: string;
  originTask: string;
  offset: string;
}

export interface AddRuleInput {
  pattern: string[];
  scope: "TASK" | "REPO" | "USER";
  /** 0 = no expiry. */
  expiresAtMs: number;
  coversAlwaysAsk: boolean;
}

export interface ContextCategoryInfo {
  category: string;
  tokens: string;
  estimatedTokens: string;
  shareBp: number;
  sources: string[];
}

export interface BudgetInfo {
  maxCostMinor: string;
  spentMinor: string;
  heldByChildrenMinor: string;
  maxWallMs: string;
  wallMsUsed: string;
  maxChildren: number;
  liveChildren: number;
  forbidSpawn: boolean;
}

/** The Core's breakdown of the request it last sent (REQ-PX-059), for the ring and its tray. */
export interface AccountingInfo {
  available: boolean;
  totalTokens: string;
  totalSource: string;
  windowTokens: string;
  windowSource: string;
  usedBp: number;
  estimatorErrorBp: number;
  declaredErrorBp: number;
  model: string;
  turnOrdinal: number;
  categories: ContextCategoryInfo[];
  compactionEpoch: number;
  compactionSummaries: number;
  budgets: BudgetInfo | null;
}

export interface CheckpointInfo {
  checkpointId: string;
  epoch: number;
  kind: string;
  status: string;
  reason: string;
  files: number;
  removed: number;
  eventOffset: string;
  createdAtMs: number;
  name: string;
  /** The turn whose boundary this is, as 32 hex characters; "" when it is not a turn's. */
  turnId: string;
  turnOrdinal: number;
  /** Why the collector keeps it: NAMED | FORK_PARENT | LATEST | PRE_RESTORE | RESTORE_TARGET | CHAIN. */
  retention: string[];
}

export interface CheckpointListInfo {
  checkpoints: CheckpointInfo[];
  currentCheckpointId: string;
  currentEpoch: number;
}

export interface RewindEntryInfo {
  path: string;
  /** WRITE | DELETE | REVERT_TO_HEAD | REMOVE_UNTRACKED | UNCHANGED. */
  action: string;
  currentHash: string;
  targetHash: string;
}

/** What a restore would do, from the Core, plus the files the person changed since the agent's last checkpoint. */
export interface RewindPreviewInfo {
  refusal: string;
  detail: string;
  checkpointId: string;
  epoch: number;
  filesWritten: number;
  filesReverted: number;
  entries: RewindEntryInfo[];
  /** Paths whose working copy differs from the task's latest checkpoint: edits made outside the agent's turns. */
  userEdited: string[];
  /** The task's current epoch when the preview was taken; a restore names it so a stale dialog is refused. */
  currentEpoch: number;
}

export interface RestoreOutcome {
  restored: boolean;
  refusal: string;
  detail: string;
  checkpointId: string;
  epoch: number;
  filesWritten: number;
  filesReverted: number;
  /** The checkpoint of the state before this restore: restoring it is the redo. */
  preRestoreCheckpointId: string;
  keptPaths: string[];
  redo: boolean;
  replayed: boolean;
}

/** Which checkpoint a preview, restore or fork is about; exactly one of the fields is set. */
export interface CheckpointTargetInput {
  checkpointId?: string;
  turnId?: string;
  turnOrdinal?: number;
  name?: string;
}

export interface ForkOutcome {
  taskId: string;
  sourceTaskId: string;
  checkpointId: string;
  turnOrdinal: number;
  filesMaterialized: number;
  approvalsDropped: number;
  worktree: string;
  branch: string;
}

export interface TaskBudgetsInfo {
  maxCostMinor: string;
  maxWallMs: string;
  maxChildren: number;
  forbidSpawn: boolean;
  offset: string;
}
