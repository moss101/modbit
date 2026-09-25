/**
 * Preload bridge (docs/32): exposes only the SurfaceProtocol functions the
 * renderer needs, over Electron's validated IPC. No Node, no fs, no shell.
 */
import { contextBridge, ipcRenderer } from "electron";

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

export interface ModbitBridge {
  coreStatus(): Promise<unknown>;
  localState(): Promise<{ sessionId?: string }>;
  languages(): Promise<{ language: string; tier: string; label: string; fixture: string; proven: string[]; provisional: string[]; notClaimed: string[]; note: string }[]>;
  contextInspector(taskId: string): Promise<ContextInspectorSummary>;
  taskEconomics(taskId: string): Promise<TaskEconomicsSummary>;
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
  createTask(sessionId: string, goal: string, commandIdHex: string, workspaceRoot?: string, issueUrl?: string): Promise<{ taskId: string; offset: bigint; replayed: boolean; goalText: string }>;
  startTask(sessionId: string, taskId: string): Promise<{ runId: string; resumed: boolean; endpoint: string; model: string }>;
  attachFile(sessionId: string, taskId: string, filePath: string): Promise<{ attachmentId: string; kind: string; mime: string; contentRef: string; offset: string; replayed: boolean }>;
  reviewBundle(taskId: string): Promise<ReviewBundleView>;
  codeView(taskId: string, path: string, expectedFileRevision?: string): Promise<CodeView>;
  decideReview(sessionId: string, taskId: string, decision: "ACCEPT" | "RETURN", rejected: { path: string; index: number }[], note: string, expectedWorkspaceRevision: string): Promise<{ taskState: string; commit: string; reverted: string[]; workspaceRevision: string }>;
  /** PX-005: a one-hunk direct edit through the Core's ChangeTransaction, bound to the revisions the review showed. */
  applyUserPatch(sessionId: string, taskId: string, patch: { path: string; old: string; new: string; expectedWorkspaceRevision: string; expectedFileRevision: string }): Promise<{ workspaceRevision: string; previousRevision: string; fileRevision: string; beforeHash: string; matchTier: string; offset: string; replayed: boolean }>;
  /** PX-007: open or update the task's pull request from the accepted candidate; APPROVAL_PENDING first, then OPENED/UPDATED or DENIED. */
  openPullRequest(sessionId: string, taskId: string, expectedCandidateRevision: string, update: boolean): Promise<PullRequestAckView>;
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
  describeBrowser(browserSessionId: string): Promise<{ browserSessionId: string; taskId: string; partition: string; attached: boolean; shown: boolean; url: string; title: string; stateVersion: number; leaseGeneration: number; controller: "AGENT" | "USER" } | null>;
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
  browserLog(): Promise<{ browserSessionId: string; kind: string; ok: boolean; code: string; generation: number; atMs: number }[]>;
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
  providerStatus(): Promise<{ configured: boolean; endpoints: string[]; stored: boolean; provider: string; keychainAvailable: boolean }>;
  trustRepository(sessionId: string, workspaceRoot: string): Promise<{ workspaceRoot: string; offset: string }>;
  starterTasks(workspaceRoot: string): Promise<{ stacks: string[]; tasks: { id: string; title: string; goalText: string; stack: string }[] }>;
}

const bridge: ModbitBridge = {
  coreStatus: () => ipcRenderer.invoke("core:status"),
  localState: () => ipcRenderer.invoke("core:localState"),
  languages: () => ipcRenderer.invoke("languages:list"),
  contextInspector: (taskId: string) => ipcRenderer.invoke("context:inspector", taskId),
  taskEconomics: (taskId: string) => ipcRenderer.invoke("task:economics", taskId),
  dashboard: (sessionId: string) => ipcRenderer.invoke("dashboard:get", sessionId),
  taskStatus: (taskId: string) => ipcRenderer.invoke("task:status", taskId),
  attention: (sessionId: string) => ipcRenderer.invoke("attention:list", sessionId),
  setTaskSelection: (sessionId: string, taskId: string, selection: unknown) => ipcRenderer.invoke("task:select", sessionId, taskId, selection),
  createSession: () => ipcRenderer.invoke("session:create"),
  sessionSnapshot: (sessionId) => ipcRenderer.invoke("session:snapshot", sessionId),
  createTask: (sessionId, goal, commandIdHex, workspaceRoot, issueUrl) => ipcRenderer.invoke("task:create", sessionId, goal, commandIdHex, workspaceRoot ?? "", issueUrl ?? ""),
  startTask: (sessionId, taskId) => ipcRenderer.invoke("task:start", sessionId, taskId),
  attachFile: (sessionId, taskId, filePath) => ipcRenderer.invoke("task:attach", sessionId, taskId, filePath),
  reviewBundle: (taskId) => ipcRenderer.invoke("review:bundle", taskId),
  codeView: (taskId, path, expectedFileRevision) => ipcRenderer.invoke("review:codeView", taskId, path, expectedFileRevision ?? ""),
  decideReview: (sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision) => ipcRenderer.invoke("review:decide", sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision),
  applyUserPatch: (sessionId, taskId, patch) => ipcRenderer.invoke("review:patch", sessionId, taskId, patch),
  openPullRequest: (sessionId, taskId, expectedCandidateRevision, update) => ipcRenderer.invoke("review:pullRequest", sessionId, taskId, expectedCandidateRevision, update),
  resolveApproval: (sessionId, approvalId, approve, reason, intentHash) => ipcRenderer.invoke("approval:resolve", sessionId, approvalId, approve, reason, intentHash),
  respondToQuestion: (sessionId, taskId, questionId, optionId, text) => ipcRenderer.invoke("question:respond", sessionId, taskId, questionId, optionId, text),
  cancelTask: (sessionId, taskId) => ipcRenderer.invoke("task:cancel", sessionId, taskId),
  steerTask: (sessionId, taskId, text) => ipcRenderer.invoke("task:steer", sessionId, taskId, text),
  openBrowser: (sessionId, taskId) => ipcRenderer.invoke("browser:open", sessionId, taskId),
  showBrowser: (browserSessionId, bounds) => ipcRenderer.invoke("browser:show", browserSessionId, bounds),
  hideBrowser: (browserSessionId) => ipcRenderer.invoke("browser:hide", browserSessionId),
  closeBrowser: (browserSessionId) => ipcRenderer.invoke("browser:close", browserSessionId),
  describeBrowser: (browserSessionId) => ipcRenderer.invoke("browser:describe", browserSessionId),
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
