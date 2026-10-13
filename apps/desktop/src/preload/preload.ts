/**
 * Preload bridge (docs/32): exposes only the SurfaceProtocol functions the
 * renderer needs, over Electron's validated IPC. No Node, no fs, no shell.
 */
import { contextBridge, ipcRenderer, webUtils } from "electron";
import type { AgentHeaderView, AgentHeadersView, ConversationSearchView, OpenStreamView, PendingApprovalView, SearchOptions, TranscriptOptions, TranscriptPageView } from "../shared/conversation-types.ts";
import type { TerminalFrameJson, TerminalViewJson } from "../main/terminal-host.ts";
import type { AttachmentResult, InterruptView, ModeChangedView, ModelCatalogView, PostureView, PreferencePatch, PreferenceSetView, QueuedChangeView, QueueView, SendBehaviorState, SideAnswerView, SlashInventoryView, TaskModeId, InputModeId } from "../shared/composer-types.ts";
import type { AutoDetail, AutoEnableInput, AutoKillReport, AutoList, AutoRepoLoad, AutoRun, AutoRunInput, AutoRunStarted, AutoValidation, AutoView } from "../shared/automation-types.ts";
import type { ApplyAckInfo, ApplyInputArgs, CleanupReportInfo, ProjectChangedInfo, ProjectInput, ProjectListInfo, ProjectPatch, WorktreeListInfo, WorktreeRemovalInfo } from "../shared/project-types.ts";
import type { AccountingInfo, AddRuleInput, AllowRuleInfo, CheckpointListInfo, CheckpointTargetInput, DockApprovalView, ExpiredApprovalView, ForkOutcome, ModeProposalDecisionInfo, ModeProposalListInfo, RestoreOutcome, RewindPreviewInfo, RunModeInfo, TaskBudgetsInfo } from "../shared/control-types.ts";

export type { TerminalFrameJson, TerminalViewJson };

export interface ReviewHunk {
  index: number;
  header: string;
  lines: string[];
}
export interface ReviewFileView {
  path: string;
  status: string;
  binary: boolean;
  fileRevision: string;
  oldContentRef: string;
  newContentRef: string;
  hunks: ReviewHunk[];
}
export interface ReviewBundleView {
  taskId: string;
  taskState: string;
  workspaceRevision: string;
  baseCommit: string;
  workspaceRoot: string;
  files: ReviewFileView[];
  planJson: string;
  selfReviewJson: string;
  verificationRuns: { id: string; stage: string; status: string; candidateRevision: string; checks: { id: string; status: string }[] }[];
  attributions: string[];
  quarantined: string[];
  invariantFindings: string[];
  receipts: number;
  evidenceLinks: string[];
  /** PX-127: the forge's CI runs for the pushed commit — external evidence with provenance `ci`, never a verification result. */
  ciEvidence: { name: string; runId: string; status: string; conclusion: string; url: string; completedAt: string; logRef: string; logTruncated: boolean; commit: string; provenance: string }[];
  ciRejected: { name: string; headSha: string; reason: string }[];
  /** PX-127: the pull request's comments the task has seen — untrusted text, what became of each. */
  reviewComments: { commentId: string; kind: string; author: string; url: string; path: string; line: string; body: string; trust: string; disposition: string; reason: string; inputId: string; answered: boolean; reportedBack: boolean; createdAt: string }[];
}
export interface TaskStatusView {
  state: string;
  waitReason: string;
  runState: string;
  loopAlive: boolean;
  lastOffset: string;
  attentionReason: string;
  failureClass: string;
  failureCode: string;
  retryable: boolean;
  userAction: string;
  recoveryPath: string;
  evidenceRefs: string[];
}
/** M7.8: a credential the broker holds — never its value. */
export interface CredentialHandle {
  handle: string;
  label: string;
  origin: string;
  username: string;
  /** false = the OS offered no encryption; the secret lives in memory only until the app quits. */
  persisted: boolean;
}

export interface BrowserSessionSummary {
  browserSessionId: string;
  taskId: string;
  partition: string;
  controller: string;
  leaseGeneration: string;
  hostAttached: boolean;
  hostKind: string;
  url: string;
  title: string;
  stateVersion: string;
  fingerprint: string;
  closed: boolean;
}
export interface PullRequestAckView {
  status: string;
  approvalId: string;
  intentHash: string;
  number: string;
  url: string;
  branch: string;
  headSha: string;
  candidateRevision: string;
  receipts: number;
  replayed: boolean;
  detail: string;
}
export interface CodeView {
  workspaceRevision: string;
  fileRevision: string;
  path: string;
  contentRef: string;
  syntaxLanguage: string;
  changedRanges: number[];
  evidenceLinks: string[];
  stale: boolean;
  text: string;
}

export interface ContextInspectorEntry {
  entryId: string;
  sourceRef: string;
  path: string;
  lineStart: number;
  lineEnd: number;
  reason: string;
  sources: string[];
  freshness: string;
  tokenCost: number;
  injected: boolean;
  used: boolean;
  stub: boolean;
}

export interface ContextInspectorSummary {
  packId: string;
  workspaceRevision: string;
  tokenBudget: number;
  tokenUsed: number;
  complete: boolean;
  injectedTokens: string;
  injectedRefs: string[];
  rejectedRefs: string[];
  omittedCount: number;
  omittedPaths: string[];
  entries: ContextInspectorEntry[];
  /** REQ-PX-059: the Core's breakdown of the last compiled request by category; null before the first compiled turn. */
  accounting?: { totalTokens: string; totalSource: string; windowTokens: string; windowSource: string; usedBp: number; model: string; categories: { category: string; tokens: string; shareBp: number }[] } | null;
  // Compaction epochs and what they cost the prompt cache (docs/19).
  compactionEpoch: number;
  compactionEpochs: number;
  compactedEntries: string;
  manifestRef: string;
  prefixCacheHits: number;
  prefixCacheMisses: number;
}

/** M10.1: one session's operations picture, from the Core's log. Counts
 * of tokens and money are decimal strings (they are 64-bit on the wire). */
export interface DashboardSummary {
  generatedAtMs: string;
  tasksByState: Record<string, number>;
  tools: { succeeded: number; failed: number; unknownOutcome: number; cancelled: number };
  cost: { minor: string; currency: string; scale: number; priced: number; unpriced: number; unreported: number };
  tokens: { input: string; cached: string; output: string };
  models: { model: string; calls: number; inputTokens: string; outputTokens: string; costMinor: string; unpricedCalls: number }[];
  tasks: { taskId: string; state: string; modelCalls: number; inputTokens: string; outputTokens: string; costMinor: string; costComplete: boolean; toolCalls: number; starts: number }[];
  slo: { cold: SloFigures; warm: SloFigures };
  failures: { offset: string; taskId: string; eventType: string; class: string; code: string }[];
  providers: string[];
}

export interface SloFigures {
  starts: string;
  readyP50Ms: string;
  readyMaxMs: string;
  firstTokenP50Ms: string;
  firstTokenMaxMs: string;
}

export interface TaskEconomicsSummary {
  state: string;
  verified: boolean;
  checksPassed: number;
  checksFailed: number;
  model: string;
  modelCalls: number;
  inputTokens: string;
  cachedInputTokens: string;
  outputTokens: string;
  costUsd: number;
  pricingKnown: boolean;
  toolCalls: number;
  wallMs: string;
  modelMs: string;
  toolMs: string;
  prefixCacheHits: number;
  prefixCacheMisses: number;
  compactionEpochs: number;
  contextTokensInjected: string;
}

/** One thing a person must act on (REQ-EV-0151 / 0275), from the Core. */
export interface AttentionItem {
  kind: string;
  taskId: string;
  reference: string;
  reason: string;
  action: string;
  sinceOffset: string;
}

/** What the host says of one browser view (PX-073 adds the reclaim, the certificate wait and the observer's counter). */
export interface BrowserViewDescription {
  browserSessionId: string;
  taskId: string;
  partition: string;
  attached: boolean;
  shown: boolean;
  url: string;
  title: string;
  stateVersion: number;
  leaseGeneration: number;
  controller: "AGENT" | "USER";
  stopped: string | null;
  humanInputAt: number;
  webContentsId: number;
  osProcessId: number;
  reclaimed: boolean;
  certPending: { id: string; hostPort: string; url: string; error: string; issuer: string; subject: string; validStart: number; validExpiry: number; fingerprint: string } | null;
  changeSeq: number;
  refusals: number;
  useSeq: number;
}

export interface WorkspaceEntry {
  path: string;
  name: string;
  kind: string;
  size: string;
  protected: boolean;
}
export interface ReceiptView {
  effectId: string;
  toolCallId: string;
  intentHash: string;
  policyDecision: string;
  executionTarget: string;
  evidenceRef: string;
  status: string;
  occurredAtMs: number;
  receiptHash: string;
  reversibility: string;
}

export interface ModbitBridge {
  /** PX-046: the header of every task of a session, with the Core's status class (the renderer never recomputes it). */
  agentHeaders(sessionId: string, includeArchived?: boolean): Promise<AgentHeadersView>;
  /** PX-047: one page of a task's conversation, projected by the Core in one of three densities. */
  transcript(taskId: string, options?: TranscriptOptions): Promise<TranscriptPageView>;
  /** PX-047: the protected effects of the session still waiting for a decision, with the exact intent each decision must name. */
  pendingApprovals(sessionId: string): Promise<PendingApprovalView[]>;
  /** PX-047: the text of the task's assistant streams that are still open, as main has seen them (a transcript carries a stream's text only once it completes). */
  openStreams(taskId: string): Promise<OpenStreamView[]>;
  /** PX-046: full-text search over the session's conversations (headers and bodies); snippets are plain data. */
  searchConversations(sessionId: string, query: string, options?: SearchOptions): Promise<ConversationSearchView>;
  /** PX-046: the person has seen the conversation up to an offset (decimal string; omitted = all of it). */
  markRead(sessionId: string, taskId: string, upToOffset?: string): Promise<{ taskId: string; readOffset: string; offset: string }>;
  /** PX-046: archive a task's conversation, or undo it with archived = false; the Core refuses a running task. */
  archiveTask(sessionId: string, taskId: string, archived: boolean): Promise<{ taskId: string; archived: boolean; offset: string }>;
  // REQ-PX-063 / 064 (projects): the Core's records and membership map. Each mutation names the acting session and may carry a 32-hex command id so a retry acts once; a refusal is the Core's typed code.
  listProjects(options?: { workspaceRoot?: string; includeArchived?: boolean }): Promise<ProjectListInfo>;
  getProject(projectId: string): Promise<ProjectChangedInfo>;
  createProject(sessionId: string, input: ProjectInput, commandId?: string): Promise<ProjectChangedInfo>;
  renameProject(sessionId: string, projectId: string, patch: ProjectPatch, commandId?: string): Promise<ProjectChangedInfo>;
  archiveProject(sessionId: string, projectId: string, archived: boolean, commandId?: string): Promise<ProjectChangedInfo>;
  addProjectMember(sessionId: string, projectId: string, taskId: string, commandId?: string): Promise<ProjectChangedInfo>;
  removeProjectMember(sessionId: string, projectId: string, taskId: string, commandId?: string): Promise<ProjectChangedInfo>;
  // REQ-PX-068 (worktrees): the Core's worktree list and its decisions. Nothing is decided in the renderer; main runs no Git.
  listWorktrees(options?: { sessionId?: string; taskId?: string }): Promise<WorktreeListInfo>;
  /** Remove one worktree by the cleanup's rule; `dryRun` asks the Core what the removal would take with it. A worktree that is not eligible is refused with a typed code. */
  removeWorktree(sessionId: string, worktreeId: string, dryRun: boolean, commandId?: string): Promise<WorktreeRemovalInfo>;
  runWorktreeCleanup(sessionId: string, options?: { dryRun?: boolean }): Promise<CleanupReportInfo>;
  /** Classify (no option) or apply the task's worktree to its checkout; the Core answers with the plan, a conflict view, an approval to resolve, or the result. */
  applyWorktree(sessionId: string, taskId: string, input?: ApplyInputArgs): Promise<ApplyAckInfo>;
  undoApply(sessionId: string, taskId: string, applyId?: string): Promise<ApplyAckInfo>;
  discardWorktree(sessionId: string, taskId: string, reason: string, confirm: string): Promise<{ worktreeId: string; disposition: string; offset: string }>;
  coreStatus(): Promise<unknown>;
  localState(): Promise<{ sessionId?: string; platform?: { os: string; arch: string; state: string; statement: string } }>;
  languages(): Promise<{ language: string; tier: string; label: string; fixture: string; proven: string[]; provisional: string[]; notClaimed: string[]; note: string }[]>;
  contextInspector(taskId: string): Promise<ContextInspectorSummary>;
  taskEconomics(taskId: string): Promise<TaskEconomicsSummary>;
  // REQ-PX-048: the apps panel's Terminal, Files and Evidence apps. Cursors are decimal strings (64-bit on the wire).
  terminalList(taskId?: string): Promise<{ terminals: TerminalViewJson[]; nowMs: number; defaultWindowBytes: string; stallMs: string }>;
  terminalAttach(sessionId: string, taskId: string, terminalId: string, afterCursor: string, opts?: { windowBytes?: number; takeInputLease?: boolean; stealInputLease?: boolean }): Promise<{ attachId: string; terminal: TerminalViewJson; afterCursor: string; windowBytes: string; inputLeaseHeld: boolean }>;
  /** The renderer has consumed the output up to `cursor`; the Core may send that much more. */
  terminalAck(attachId: string, cursor: string): Promise<string>;
  terminalDetach(attachId: string): Promise<{ wasAttached: boolean; ackedCursor: string }>;
  /** Take (hold) or give back the person's input lease on an attachment. */
  terminalInput(attachId: string, hold: boolean, steal?: boolean): Promise<{ held: boolean; holder: string; offset: string }>;
  terminalResize(attachId: string, rows: number, cols: number): Promise<{ rows: number; cols: number }>;
  terminalWrite(attachId: string, data: Uint8Array | string): Promise<{ bytes: string; cursor: string }>;
  terminalKill(sessionId: string, taskId: string, terminalId: string): Promise<{ outcome: string; exitCode: number | null }>;
  onTerminalFrame(cb: (f: TerminalFrameJson) => void): () => void;
  listWorkspaceDir(taskId: string, path: string): Promise<{ path: string; workspaceRevision: string; truncated: boolean; entries: WorkspaceEntry[] }>;
  readWorkspaceFile(taskId: string, path: string): Promise<{ path: string; status: string; size: string; text: string; fileRevision: string; workspaceRevision: string; language: string }>;
  effectReceipts(taskId: string): Promise<{ chainValid: boolean; detail: string; receipts: ReceiptView[] }>;
  /** REQ-PX-058: the protected effects waiting for a decision (oldest first), each with the Core's typed reason and the exact intent; with a task id, that task's only. */
  dockApprovals(sessionId: string, taskId?: string): Promise<DockApprovalView[]>;
  /** REQ-PX-058: the task's approvals the Core closed as expired (nobody answered before the recorded expiry). They can no longer be approved. */
  expiredApprovals(sessionId: string, taskId: string): Promise<ExpiredApprovalView[]>;
  /** REQ-PX-055: the agent's requests to change the task's mode, with the Core's clock. Data until the person answers. */
  modeProposals(taskId: string): Promise<ModeProposalListInfo>;
  /** REQ-PX-055: the person's answer; accepting moves the mode through the Core's one mode path. */
  decideModeProposal(sessionId: string, taskId: string, proposalId: string, accept: boolean): Promise<ModeProposalDecisionInfo>;
  /** REQ-PX-058 / 057: make a durable argv-prefix rule through the Core, then approve this one effect, named by the hash the card showed; the rule is not made if the effect is no longer the one shown. */
  allowAlways(sessionId: string, approvalId: string, intentHash: string, rule: AddRuleInput): Promise<{ rule: AllowRuleInfo; approvalId: string; status: string; offset: string }>;
  /** REQ-PX-057: the run mode in force, the always-ask classes and the warning a move to a higher mode must acknowledge. */
  runMode(taskId: string): Promise<RunModeInfo>;
  setRunMode(sessionId: string, taskId: string, mode: string, acknowledgeRisk: boolean): Promise<RunModeInfo>;
  allowRules(taskId: string, includeInactive?: boolean): Promise<AllowRuleInfo[]>;
  addAllowRule(sessionId: string, taskId: string, rule: AddRuleInput): Promise<AllowRuleInfo>;
  revokeAllowRule(sessionId: string, taskId: string, ruleId: string, reason?: string): Promise<{ ruleId: string; offset: string }>;
  /** REQ-PX-060: the Core's breakdown of the last request by category, the model's window and the budgets beside them. */
  contextAccounting(taskId: string): Promise<AccountingInfo>;
  /** REQ-PX-116 / 060: raise (or set) the task's cost and wall-clock limits; counts are decimal strings, 0 = none. */
  setTaskBudgets(sessionId: string, taskId: string, budgets: { maxCostMinor?: string; maxWallMs?: string }): Promise<TaskBudgetsInfo>;
  /** REQ-PX-062: the task's checkpoints, a preview of what restoring one would change, the restore itself (reversible), and a fork from a turn. */
  checkpoints(taskId: string): Promise<CheckpointListInfo>;
  previewRestore(taskId: string, target: CheckpointTargetInput): Promise<RewindPreviewInfo>;
  restoreCheckpoint(sessionId: string, taskId: string, target: CheckpointTargetInput, options?: { expected?: { path: string; contentHash: string }[]; keepPaths?: string[]; redo?: boolean; expectedCurrentEpoch?: number; commandId?: string }): Promise<RestoreOutcome>;
  forkFromTurn(sessionId: string, taskId: string, target: CheckpointTargetInput, goal?: string, commandId?: string): Promise<ForkOutcome>;
  /** REQ-PX-086: the automations surface. Every call is the Core's own command; a refusal arrives as `CODE: message`. */
  automations(): Promise<AutoList>;
  automation(automationId: string): Promise<AutoDetail>;
  automationRuns(options?: { automationId?: string; taskId?: string; limit?: number }): Promise<AutoRun[]>;
  /** The editor's live validation: the Core's parser and validator; nothing is stored. */
  validateAutomation(definitionJson: string): Promise<AutoValidation>;
  createAutomation(definitionJson: string, workspaceRoot: string, commandId?: string): Promise<AutoView>;
  updateAutomation(automationId: string, definitionJson: string, commandId?: string): Promise<AutoView>;
  loadRepositoryAutomations(workspaceRoot: string): Promise<AutoRepoLoad>;
  /** The owner's approval: names exactly the version, hash and lists the dialog showed. */
  enableAutomation(approval: AutoEnableInput, commandId?: string): Promise<AutoView>;
  disableAutomation(automationId: string, note?: string): Promise<AutoView>;
  pauseAutomation(automationId: string, paused: boolean, note?: string): Promise<{ view: AutoView | null; list: AutoList | null }>;
  killAutomation(automationId: string, note?: string): Promise<AutoKillReport>;
  runAutomation(request: AutoRunInput, commandId?: string): Promise<AutoRunStarted>;
  ackAutomationAttention(dispatchKey: string): Promise<{ acknowledged: boolean }>;
  /** AUT-E03: the headers of tasks automations created (each run is its own session). */
  automationTaskHeaders(): Promise<AgentHeaderView[]>;
  /** M10.1: the session's telemetry, cost and SLO dashboard. */
  dashboard(sessionId: string): Promise<DashboardSummary>;
  /** PX-023: the typed task status (REQ-EV-0073) a snapshot does not carry. */
  taskStatus(taskId: string): Promise<TaskStatusView>;
  attention(sessionId: string): Promise<{ lastOffset: string; items: AttentionItem[] }>;
  setTaskSelection(
    sessionId: string,
    taskId: string,
    selection: { paths: string[]; symbol?: string; lineStart?: number; lineEnd?: number; reviewHunks: string[]; source: string },
  ): Promise<{ offset: string }>;
  createSession(): Promise<string>;
  sessionSnapshot(sessionId: string): Promise<unknown>;
  createTask(sessionId: string, goal: string, commandIdHex: string, workspaceRoot?: string, issueUrl?: string, options?: { isolation?: "NONE" | "WORKTREE" }): Promise<{ taskId: string; offset: bigint; replayed: boolean; goalText: string }>;
  /** `skills` are the names the person chose in the slash menu; the Core decides whether each may reach the model. */
  startTask(sessionId: string, taskId: string, options?: { skills?: string[] }): Promise<{ runId: string; resumed: boolean; endpoint: string; model: string }>;
  /** REQ-PX-054: attach a file the person chose, dropped or pasted. The preload resolves a chosen File's path itself; main decides the type from the bytes and the Core ingests it. A string path is the Fleet board's earlier contract, kept for it and now type-checked by main like any other. */
  attachFile(sessionId: string, taskId: string, file: File | string): Promise<AttachmentResult>;
  // REQ-PX-054..056: the composer. Each is one typed Core command or read; the renderer decides nothing a command decides.
  composerPosture(taskId: string): Promise<PostureView>;
  composerSetMode(sessionId: string, taskId: string, mode: TaskModeId): Promise<ModeChangedView>;
  composerSetPreference(sessionId: string, taskId: string, patch: PreferencePatch): Promise<PreferenceSetView>;
  composerVariants(taskId?: string): Promise<ModelCatalogView>;
  composerSlash(taskId?: string): Promise<SlashInventoryView>;
  composerQueue(taskId: string): Promise<QueueView>;
  /** `mode` DEFAULT is plain Enter: the Core applies the task's send behaviour. `inputId` is client-stable; a retry replays. */
  composerQueueInput(sessionId: string, taskId: string, text: string, mode: InputModeId, inputId: string): Promise<{ inputId: string; sequence: string; offset: string }>;
  composerEditQueued(sessionId: string, taskId: string, inputId: string, change: { text?: string; mode?: string }): Promise<QueuedChangeView>;
  composerRemoveQueued(sessionId: string, taskId: string, inputId: string): Promise<QueuedChangeView>;
  composerReorderQueued(sessionId: string, taskId: string, inputId: string, beforeInputId?: string): Promise<QueuedChangeView>;
  composerSendNow(sessionId: string, taskId: string, inputId: string, interruptId?: string): Promise<InterruptView>;
  composerInterrupt(sessionId: string, taskId: string, interruptId?: string): Promise<InterruptView>;
  composerSendBehavior(taskId: string): Promise<SendBehaviorState>;
  composerSetSendBehavior(sessionId: string, taskId: string, whileRunning: string, sendNow: string): Promise<SendBehaviorState>;
  composerSideQuestion(taskId: string, text: string): Promise<SideAnswerView>;
  reviewBundle(taskId: string): Promise<ReviewBundleView>;
  codeView(taskId: string, path: string, expectedFileRevision?: string): Promise<CodeView>;
  decideReview(sessionId: string, taskId: string, decision: "ACCEPT" | "RETURN", rejected: { path: string; index: number }[], note: string, expectedWorkspaceRevision: string): Promise<{ taskState: string; commit: string; reverted: string[]; workspaceRevision: string }>;
  /** PX-005: a one-hunk direct edit through the Core's ChangeTransaction, bound to the revisions the review showed. */
  applyUserPatch(sessionId: string, taskId: string, patch: { path: string; old: string; new: string; expectedWorkspaceRevision: string; expectedFileRevision: string }): Promise<{ workspaceRevision: string; previousRevision: string; fileRevision: string; beforeHash: string; matchTier: string; offset: string; replayed: boolean }>;
  /** PX-007: open or update the task's pull request from the accepted candidate; APPROVAL_PENDING first, then OPENED/UPDATED or DENIED. */
  openPullRequest(sessionId: string, taskId: string, expectedCandidateRevision: string, update: boolean): Promise<PullRequestAckView>;
  /** PX-127: the Core reads the forge's check runs for the task's pull request and records them as `ci` evidence. */
  ingestCiResults(sessionId: string, taskId: string): Promise<{ commit: string; checks: number; refused: number; evidenceClass: string; offset: string }>;
  /** PX-127: the Core reads the pull request's comments; allowed authors' comments addressed to Modbit steer as untrusted input. */
  ingestReviewComments(sessionId: string, taskId: string): Promise<{ steered: number; ignored: number; alreadyTaken: number; offset: string }>;
  /** PX-001: decide a protected effect, naming the intent hash shown. */
  resolveApproval(sessionId: string, approvalId: string, approve: boolean, reason: string, intentHash: string): Promise<{ approvalId: string; status: string; offset: string }>;
  /** PX-024: cancel the task (confirmed in the renderer) and steer it with one line of input. */
  cancelTask(sessionId: string, taskId: string): Promise<{ wasRunning: boolean }>;
  steerTask(sessionId: string, taskId: string, text: string): Promise<{ sequence: string; offset: string }>;
  /** REQ-EV-0222: answer the agent's typed question (option id or free text); resume with startTask. */
  respondToQuestion(sessionId: string, taskId: string, questionId: string, optionId: string, text: string): Promise<{ questionId: string; alreadyAnswered: boolean }>;
  /** M7.1 (docs/22): the task's browser session — opened on the Core, hosted by main's sandboxed view, placed where the renderer says. */
  openBrowser(sessionId: string, taskId: string): Promise<{ browserSessionId: string; partition: string; leaseGeneration: string }>;
  showBrowser(browserSessionId: string, bounds: { x: number; y: number; width: number; height: number }): Promise<boolean>;
  hideBrowser(browserSessionId: string): Promise<void>;
  closeBrowser(browserSessionId: string): Promise<void>;
  describeBrowser(browserSessionId: string, taskId?: string): Promise<BrowserViewDescription | null>;
  /** PX-073: the views a task owns, and nobody else's. */
  listBrowserViews(taskId: string): Promise<{ browserSessionId: string; url: string; shown: boolean; reclaimed: boolean }[]>;
  selectBrowserView(taskId: string, browserSessionId: string): Promise<BrowserViewDescription | null>;
  /** PX-073 / PX-120: the person's browser policy (local destinations and origins the sessions may use). */
  browserPolicy(): Promise<{ version: number; allowTargets: string[]; allowOrigins: string[] }>;
  setBrowserPolicy(next: { allowTargets?: string[]; allowOrigins?: string[] }): Promise<{ version: number; allowTargets: string[]; allowOrigins: string[] }>;
  /** PX-120: destinations the target policy refused in a session. */
  browserRefusals(browserSessionId: string): Promise<{ atMs: number; host: string; address: string; class: string; reason: string; document: boolean; main: boolean; url: string }[]>;
  /** PX-073: decide a held certificate error; trusts are remembered per workspace and can be listed and cleared. */
  decideBrowserCertificate(browserSessionId: string, id: string, decision: "trust" | "reject"): Promise<boolean>;
  browserCertificateTrusts(): Promise<{ workspace: string; hostPort: string; fingerprint: string; issuer: string; subject: string; validExpiry: number; trustedAtMs: number }[]>;
  clearBrowserCertificateTrusts(): Promise<number>;
  /** PX-073: sign out of the sites a session visited (cookies, storage, service workers, caches). */
  clearBrowserData(browserSessionId?: string): Promise<{ cleared: string[] }>;
  /** M7.6: take (USER) or return (AGENT) control of the session; the lease generation moves on every hand-over. */
  setBrowserControl(browserSessionId: string, controller: "AGENT" | "USER"): Promise<{ controller: string; leaseGeneration: number; changed: boolean }>;
  typeAsPerson(browserSessionId: string, text: string): Promise<boolean>;
  browserSession(browserSessionId: string, taskId: string): Promise<BrowserSessionSummary>;
  /** IMP-EV-0085: the emergency stop — the host halts agent input at once, the Core blocks every new effect and revokes the leases. */
  emergencyStop(sessionId: string, reason: string): Promise<{ leasesRevoked: number; offset: string }>;
  /** M7.8: the credential broker — add binds a secret to an origin (the secret crosses once; a handle comes back); list and remove never carry a secret. */
  addCredential(label: string, origin: string, username: string, secret: string): Promise<CredentialHandle>;
  listCredentials(): Promise<CredentialHandle[]>;
  removeCredential(handle: string): Promise<{ removed: boolean }>;
  browserLog(): Promise<{ browserSessionId: string; kind: string; ok: boolean; code: string; generation: number; atMs: number; detail?: string }[]>;
  /** The isolation report asked of the hosted page itself (Node, require, Electron: none reachable). */
  probeBrowser(browserSessionId: string): Promise<{ kind: string; node_reachable: boolean; partition: string; sandboxed: boolean; context_isolated: boolean } | null>;
  onBrowserState(cb: (s: unknown) => void): () => void;
  /** PX-023: hand one notification to the OS (main keeps a delivery log). */
  deliverNotification(id: string, title: string, body: string): Promise<{ shown: boolean }>;
  notificationLog(): Promise<{ id: string; title: string; body: string; atMs: number; shown: boolean }[]>;
  subscribe(sessionId: string, afterOffset: string): Promise<void>;
  onEvent(cb: (e: unknown) => void): () => void;
  onCoreStatus(cb: (s: unknown) => void): () => void;
  onRecovery(cb: (r: unknown) => void): () => void;
  debugCoreInfo(): Promise<{ pid: number; endpoint: string } | null>;
  /** REQ-EV-0075: messages main refused because they did not come from this app's top frame. */
  debugIpcRefusals(): Promise<{ channel: string; senderId: number; frameUrl: string; reason: string; atMs: number }[]>;
  /** REQ-EV-0076: the renderer's own process endings and whether main loaded it again. */
  debugRendererLog(): Promise<{ reason: string; exitCode: number; reloaded: boolean; atMs: number }[]>;
  // Onboarding (REQ-PX-022). The key goes to main and never comes back.
  setupProvider(provider: "openai" | "anthropic", apiKey: string, baseUrl?: string): Promise<{ endpoint: string; model: string; ok: boolean; errorCode: string; errorMessage: string; persisted: boolean; keychainAvailable: boolean }>;
  providerStatus(): Promise<{ configured: boolean; endpoints: string[]; stored: boolean; provider: string; keychainAvailable: boolean; keychainBackend: string }>;
  trustRepository(sessionId: string, workspaceRoot: string): Promise<{ workspaceRoot: string; offset: string }>;
  starterTasks(workspaceRoot: string): Promise<{ stacks: string[]; tasks: { id: string; title: string; goalText: string; stack: string }[] }>;
}

const bridge: ModbitBridge = {
  agentHeaders: (sessionId, includeArchived) => ipcRenderer.invoke("agents:headers", sessionId, includeArchived ?? false),
  transcript: (taskId, options) => ipcRenderer.invoke("conversation:transcript", taskId, options ?? {}),
  pendingApprovals: (sessionId) => ipcRenderer.invoke("conversation:approvals", sessionId),
  openStreams: (taskId) => ipcRenderer.invoke("conversation:openStreams", taskId),
  searchConversations: (sessionId, query, options) => ipcRenderer.invoke("conversation:search", sessionId, query, options ?? {}),
  markRead: (sessionId, taskId, upToOffset) => ipcRenderer.invoke("conversation:markRead", sessionId, taskId, upToOffset ?? ""),
  archiveTask: (sessionId, taskId, archived) => ipcRenderer.invoke("conversation:archive", sessionId, taskId, archived),
  listProjects: (options) => ipcRenderer.invoke("projects:list", options ?? {}),
  getProject: (projectId) => ipcRenderer.invoke("projects:get", projectId),
  createProject: (sessionId, input, commandId) => ipcRenderer.invoke("projects:create", sessionId, input, commandId ?? ""),
  renameProject: (sessionId, projectId, patch, commandId) => ipcRenderer.invoke("projects:rename", sessionId, projectId, patch, commandId ?? ""),
  archiveProject: (sessionId, projectId, archived, commandId) => ipcRenderer.invoke("projects:archive", sessionId, projectId, archived, commandId ?? ""),
  addProjectMember: (sessionId, projectId, taskId, commandId) => ipcRenderer.invoke("projects:addMember", sessionId, projectId, taskId, commandId ?? ""),
  removeProjectMember: (sessionId, projectId, taskId, commandId) => ipcRenderer.invoke("projects:removeMember", sessionId, projectId, taskId, commandId ?? ""),
  listWorktrees: (options) => ipcRenderer.invoke("worktrees:list", options ?? {}),
  removeWorktree: (sessionId, worktreeId, dryRun, commandId) => ipcRenderer.invoke("worktrees:remove", sessionId, worktreeId, dryRun, commandId ?? ""),
  runWorktreeCleanup: (sessionId, options) => ipcRenderer.invoke("worktrees:cleanup", sessionId, options ?? {}),
  applyWorktree: (sessionId, taskId, input) => ipcRenderer.invoke("worktrees:apply", sessionId, taskId, input ?? {}),
  undoApply: (sessionId, taskId, applyId) => ipcRenderer.invoke("worktrees:undo", sessionId, taskId, applyId ?? ""),
  discardWorktree: (sessionId, taskId, reason, confirm) => ipcRenderer.invoke("worktrees:discard", sessionId, taskId, reason, confirm),
  coreStatus: () => ipcRenderer.invoke("core:status"),
  localState: () => ipcRenderer.invoke("core:localState"),
  languages: () => ipcRenderer.invoke("languages:list"),
  contextInspector: (taskId: string) => ipcRenderer.invoke("context:inspector", taskId),
  taskEconomics: (taskId: string) => ipcRenderer.invoke("task:economics", taskId),
  terminalList: (taskId) => ipcRenderer.invoke("terminal:list", taskId ?? ""),
  terminalAttach: (sessionId, taskId, terminalId, afterCursor, opts) => ipcRenderer.invoke("terminal:attach", sessionId, taskId, terminalId, afterCursor, opts ?? {}),
  terminalAck: (attachId, cursor) => ipcRenderer.invoke("terminal:ack", attachId, cursor),
  terminalDetach: (attachId) => ipcRenderer.invoke("terminal:detach", attachId),
  terminalInput: (attachId, hold, steal) => ipcRenderer.invoke("terminal:input", attachId, hold, steal ?? false),
  terminalResize: (attachId, rows, cols) => ipcRenderer.invoke("terminal:resize", attachId, rows, cols),
  terminalWrite: (attachId, data) => ipcRenderer.invoke("terminal:write", attachId, data),
  terminalKill: (sessionId, taskId, terminalId) => ipcRenderer.invoke("terminal:kill", sessionId, taskId, terminalId),
  onTerminalFrame: (cb) => {
    const listener = (_: unknown, f: unknown) => cb(f as TerminalFrameJson);
    ipcRenderer.on("terminal:frame", listener);
    return () => ipcRenderer.removeListener("terminal:frame", listener);
  },
  listWorkspaceDir: (taskId, path) => ipcRenderer.invoke("files:list", taskId, path),
  readWorkspaceFile: (taskId, path) => ipcRenderer.invoke("files:read", taskId, path),
  effectReceipts: (taskId) => ipcRenderer.invoke("evidence:receipts", taskId),
  dockApprovals: (sessionId, taskId) => ipcRenderer.invoke("approvals:dock", sessionId, taskId ?? ""),
  expiredApprovals: (sessionId, taskId) => ipcRenderer.invoke("approvals:expired", sessionId, taskId),
  modeProposals: (taskId) => ipcRenderer.invoke("modeproposals:list", taskId),
  decideModeProposal: (sessionId, taskId, proposalId, accept) => ipcRenderer.invoke("modeproposals:decide", sessionId, taskId, proposalId, accept),
  allowAlways: (sessionId, approvalId, intentHash, rule) => ipcRenderer.invoke("approval:allowAlways", sessionId, approvalId, intentHash, rule),
  runMode: (taskId) => ipcRenderer.invoke("runmode:get", taskId),
  setRunMode: (sessionId, taskId, mode, acknowledgeRisk) => ipcRenderer.invoke("runmode:set", sessionId, taskId, mode, acknowledgeRisk),
  allowRules: (taskId, includeInactive) => ipcRenderer.invoke("rules:list", taskId, includeInactive ?? false),
  addAllowRule: (sessionId, taskId, rule) => ipcRenderer.invoke("rules:add", sessionId, taskId, rule),
  revokeAllowRule: (sessionId, taskId, ruleId, reason) => ipcRenderer.invoke("rules:revoke", sessionId, taskId, ruleId, reason ?? ""),
  contextAccounting: (taskId) => ipcRenderer.invoke("accounting:get", taskId),
  setTaskBudgets: (sessionId, taskId, budgets) => ipcRenderer.invoke("budgets:set", sessionId, taskId, budgets),
  automations: () => ipcRenderer.invoke("automations:list"),
  automation: (id) => ipcRenderer.invoke("automations:get", id),
  automationRuns: (options) => ipcRenderer.invoke("automations:runs", options ?? {}),
  validateAutomation: (json) => ipcRenderer.invoke("automations:validate", json),
  createAutomation: (json, root, commandId) => ipcRenderer.invoke("automations:create", json, root, commandId ?? ""),
  updateAutomation: (id, json, commandId) => ipcRenderer.invoke("automations:update", id, json, commandId ?? ""),
  loadRepositoryAutomations: (root) => ipcRenderer.invoke("automations:loadRepository", root),
  enableAutomation: (approval, commandId) => ipcRenderer.invoke("automations:enable", approval, commandId ?? ""),
  disableAutomation: (id, note) => ipcRenderer.invoke("automations:disable", id, note ?? ""),
  pauseAutomation: (id, paused, note) => ipcRenderer.invoke("automations:pause", id, paused, note ?? ""),
  killAutomation: (id, note) => ipcRenderer.invoke("automations:kill", id, note ?? ""),
  runAutomation: (request, commandId) => ipcRenderer.invoke("automations:run", request, commandId ?? ""),
  ackAutomationAttention: (dispatchKey) => ipcRenderer.invoke("automations:ack", dispatchKey),
  automationTaskHeaders: () => ipcRenderer.invoke("automations:taskHeaders"),
  checkpoints: (taskId) => ipcRenderer.invoke("checkpoints:list", taskId),
  previewRestore: (taskId, target) => ipcRenderer.invoke("checkpoints:preview", taskId, target),
  restoreCheckpoint: (sessionId, taskId, target, options) => ipcRenderer.invoke("checkpoints:restore", sessionId, taskId, target, options ?? {}),
  forkFromTurn: (sessionId, taskId, target, goal, commandId) => ipcRenderer.invoke("checkpoints:fork", sessionId, taskId, target, goal ?? "", commandId ?? ""),
  dashboard: (sessionId: string) => ipcRenderer.invoke("dashboard:get", sessionId),
  taskStatus: (taskId: string) => ipcRenderer.invoke("task:status", taskId),
  attention: (sessionId: string) => ipcRenderer.invoke("attention:list", sessionId),
  setTaskSelection: (sessionId: string, taskId: string, selection: unknown) => ipcRenderer.invoke("task:select", sessionId, taskId, selection),
  createSession: () => ipcRenderer.invoke("session:create"),
  sessionSnapshot: (sessionId) => ipcRenderer.invoke("session:snapshot", sessionId),
  createTask: (sessionId, goal, commandIdHex, workspaceRoot, issueUrl, options) => ipcRenderer.invoke("task:create", sessionId, goal, commandIdHex, workspaceRoot ?? "", issueUrl ?? "", options ?? {}),
  startTask: (sessionId, taskId, options) => ipcRenderer.invoke("task:start", sessionId, taskId, options ?? {}),
  attachFile: async (sessionId, taskId, file) => {
    // A File the person chose or dropped has a path the preload can resolve; a pasted image has none, and its bytes go instead.
    if (typeof file === "string") return ipcRenderer.invoke("composer:attachPath", sessionId, taskId, file, "");
    let path = "";
    try {
      path = webUtils.getPathForFile(file);
    } catch {
      path = "";
    }
    if (path) return ipcRenderer.invoke("composer:attachPath", sessionId, taskId, path, file.type);
    return ipcRenderer.invoke("composer:attachBytes", sessionId, taskId, file.name, new Uint8Array(await file.arrayBuffer()), file.type);
  },
  composerPosture: (taskId) => ipcRenderer.invoke("composer:posture", taskId),
  composerSetMode: (sessionId, taskId, mode) => ipcRenderer.invoke("composer:setMode", sessionId, taskId, mode),
  composerSetPreference: (sessionId, taskId, patch) => ipcRenderer.invoke("composer:setPreference", sessionId, taskId, patch),
  composerVariants: (taskId) => ipcRenderer.invoke("composer:variants", taskId ?? ""),
  composerSlash: (taskId) => ipcRenderer.invoke("composer:slash", taskId ?? ""),
  composerQueue: (taskId) => ipcRenderer.invoke("composer:queue", taskId),
  composerQueueInput: (sessionId, taskId, text, mode, inputId) => ipcRenderer.invoke("composer:queueInput", sessionId, taskId, text, mode, inputId),
  composerEditQueued: (sessionId, taskId, inputId, change) => ipcRenderer.invoke("composer:editQueued", sessionId, taskId, inputId, change),
  composerRemoveQueued: (sessionId, taskId, inputId) => ipcRenderer.invoke("composer:removeQueued", sessionId, taskId, inputId),
  composerReorderQueued: (sessionId, taskId, inputId, beforeInputId) => ipcRenderer.invoke("composer:reorderQueued", sessionId, taskId, inputId, beforeInputId ?? ""),
  composerSendNow: (sessionId, taskId, inputId, interruptId) => ipcRenderer.invoke("composer:sendNow", sessionId, taskId, inputId, interruptId ?? ""),
  composerInterrupt: (sessionId, taskId, interruptId) => ipcRenderer.invoke("composer:interrupt", sessionId, taskId, interruptId ?? ""),
  composerSendBehavior: (taskId) => ipcRenderer.invoke("composer:sendBehavior", taskId),
  composerSetSendBehavior: (sessionId, taskId, whileRunning, sendNow) => ipcRenderer.invoke("composer:setSendBehavior", sessionId, taskId, whileRunning, sendNow),
  composerSideQuestion: (taskId, text) => ipcRenderer.invoke("composer:sideQuestion", taskId, text),
  reviewBundle: (taskId) => ipcRenderer.invoke("review:bundle", taskId),
  codeView: (taskId, path, expectedFileRevision) => ipcRenderer.invoke("review:codeView", taskId, path, expectedFileRevision ?? ""),
  decideReview: (sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision) => ipcRenderer.invoke("review:decide", sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision),
  applyUserPatch: (sessionId, taskId, patch) => ipcRenderer.invoke("review:patch", sessionId, taskId, patch),
  openPullRequest: (sessionId, taskId, expectedCandidateRevision, update) => ipcRenderer.invoke("review:pullRequest", sessionId, taskId, expectedCandidateRevision, update),
  ingestCiResults: (sessionId, taskId) => ipcRenderer.invoke("review:ingestCi", sessionId, taskId),
  ingestReviewComments: (sessionId, taskId) => ipcRenderer.invoke("review:ingestComments", sessionId, taskId),
  resolveApproval: (sessionId, approvalId, approve, reason, intentHash) => ipcRenderer.invoke("approval:resolve", sessionId, approvalId, approve, reason, intentHash),
  respondToQuestion: (sessionId, taskId, questionId, optionId, text) => ipcRenderer.invoke("question:respond", sessionId, taskId, questionId, optionId, text),
  cancelTask: (sessionId, taskId) => ipcRenderer.invoke("task:cancel", sessionId, taskId),
  steerTask: (sessionId, taskId, text) => ipcRenderer.invoke("task:steer", sessionId, taskId, text),
  openBrowser: (sessionId, taskId) => ipcRenderer.invoke("browser:open", sessionId, taskId),
  showBrowser: (browserSessionId, bounds) => ipcRenderer.invoke("browser:show", browserSessionId, bounds),
  hideBrowser: (browserSessionId) => ipcRenderer.invoke("browser:hide", browserSessionId),
  closeBrowser: (browserSessionId) => ipcRenderer.invoke("browser:close", browserSessionId),
  describeBrowser: (browserSessionId, taskId) => ipcRenderer.invoke("browser:describe", browserSessionId, taskId),
  listBrowserViews: (taskId) => ipcRenderer.invoke("browser:list", taskId),
  selectBrowserView: (taskId, browserSessionId) => ipcRenderer.invoke("browser:select", taskId, browserSessionId),
  browserPolicy: () => ipcRenderer.invoke("browser:policy"),
  setBrowserPolicy: (next) => ipcRenderer.invoke("browser:setPolicy", next),
  browserRefusals: (browserSessionId) => ipcRenderer.invoke("browser:refusals", browserSessionId),
  decideBrowserCertificate: (browserSessionId, id, decision) => ipcRenderer.invoke("browser:certDecide", browserSessionId, id, decision),
  browserCertificateTrusts: () => ipcRenderer.invoke("browser:certTrusts"),
  clearBrowserCertificateTrusts: () => ipcRenderer.invoke("browser:certClear"),
  clearBrowserData: (browserSessionId) => ipcRenderer.invoke("browser:clearData", browserSessionId),
  browserSession: (browserSessionId, taskId) => ipcRenderer.invoke("browser:session", browserSessionId, taskId),
  browserLog: () => ipcRenderer.invoke("browser:log"),
  probeBrowser: (browserSessionId) => ipcRenderer.invoke("browser:probe", browserSessionId),
  setBrowserControl: (browserSessionId, controller) => ipcRenderer.invoke("browser:control", browserSessionId, controller),
  typeAsPerson: (browserSessionId, text) => ipcRenderer.invoke("browser:typeAsPerson", browserSessionId, text),
  emergencyStop: (sessionId, reason) => ipcRenderer.invoke("browser:emergencyStop", sessionId, reason),
  addCredential: (label, origin, username, secret) => ipcRenderer.invoke("credential:add", label, origin, username, secret),
  listCredentials: () => ipcRenderer.invoke("credential:list"),
  removeCredential: (handle) => ipcRenderer.invoke("credential:remove", handle),
  onBrowserState: (cb) => {
    const listener = (_e: unknown, s: unknown) => cb(s);
    ipcRenderer.on("browser:state", listener);
    return () => ipcRenderer.removeListener("browser:state", listener);
  },
  deliverNotification: (id, title, body) => ipcRenderer.invoke("notify:deliver", id, title, body),
  notificationLog: () => ipcRenderer.invoke("notify:log"),
  subscribe: (sessionId, afterOffset) => ipcRenderer.invoke("events:subscribe", sessionId, afterOffset),
  onEvent: (cb) => {
    const listener = (_: unknown, e: unknown) => cb(e);
    ipcRenderer.on("core:event", listener);
    return () => ipcRenderer.removeListener("core:event", listener);
  },
  onCoreStatus: (cb) => {
    const listener = (_: unknown, s: unknown) => cb(s);
    ipcRenderer.on("core:status", listener);
    return () => ipcRenderer.removeListener("core:status", listener);
  },
  onRecovery: (cb) => {
    const listener = (_: unknown, r: unknown) => cb(r);
    ipcRenderer.on("core:recovery", listener);
    return () => ipcRenderer.removeListener("core:recovery", listener);
  },
  debugCoreInfo: () => ipcRenderer.invoke("debug:coreInfo"),
  debugIpcRefusals: () => ipcRenderer.invoke("debug:ipcRefusals"),
  debugRendererLog: () => ipcRenderer.invoke("debug:rendererLog"),
  setupProvider: (provider, apiKey, baseUrl) => ipcRenderer.invoke("onboarding:provider", provider, apiKey, baseUrl ?? ""),
  providerStatus: () => ipcRenderer.invoke("onboarding:providerStatus"),
  trustRepository: (sessionId, workspaceRoot) => ipcRenderer.invoke("onboarding:trust", sessionId, workspaceRoot),
  starterTasks: (workspaceRoot) => ipcRenderer.invoke("onboarding:starters", workspaceRoot),
};

contextBridge.exposeInMainWorld("modbit", bridge);
