/**
 * The plain, structured-clone-safe shapes the agent list and the conversation
 * cross the preload bridge as (REQ-PX-046, REQ-PX-047). Main converts the
 * Core's protobuf messages into these (64-bit offsets as decimal strings, ids
 * as hex, enums as their stable names); the renderer renders them and never
 * recomputes what the Core decided: the status class of a header and the
 * grouping of a density are the Core's (docs/65 AFW-B02, AFW-C05).
 */

/** The Core's status classes, in the Core's precedence order (docs/65 AFW-B02). */
export const STATUS_CLASSES = ["NEEDS_ATTENTION", "FAILED", "READY_FOR_REVIEW_UNSEEN", "READY_FOR_REVIEW_SEEN", "RUNNING", "WAITING", "COMPLETED", "DRAFT", "ARCHIVED"] as const;
export type StatusClass = (typeof STATUS_CLASSES)[number];

export const DENSITIES = ["COMPACT", "BALANCED", "DETAILED"] as const;
export type Density = (typeof DENSITIES)[number];

export type RowKind = "USER_MESSAGE" | "ASSISTANT_MESSAGE" | "WORK_GROUP" | "TOOL_CARD" | "APPROVAL_CARD" | "TAIL_STATUS" | "TURN_FOOTER" | "TIME_BOUNDARY" | "UNREAD_DIVIDER" | "UNSPECIFIED";

export interface AgentHeaderView {
  taskId: string;
  sessionId: string;
  workspaceRoot: string;
  title: string;
  subtitle: string;
  createdAtMs: number;
  updatedAtMs: number;
  /** The Core's class; the renderer shows it and never recomputes it. */
  statusClass: StatusClass;
  statusLabel: string;
  unread: boolean;
  pendingApproval: boolean;
  pendingPlan: boolean;
  contextPercent: number;
  filesChanged: number;
  linesAdded: number;
  linesRemoved: number;
  lastCheckpointAtMs: number;
  subagent: boolean;
  archived: boolean;
  executionLocation: string;
  origin: string;
  taskState: string;
  lastOffset: string;
  readOffset: string;
  attentionItems: number;
  /** The project the Core's membership map puts the task in (PX-063); "" when none. */
  projectId?: string;
  /** The checkout a task in a worktree of its own came from (PX-118); "" or absent when the task works in its workspace root itself. */
  checkoutRoot?: string;
  /**
   * The last non-empty line of the assistant's latest finished message (PX-218), for the row's preview card: the Core's words, redacted as
   * the transcript is, at most 200 characters (a longer line ends in an ellipsis). "" or absent when the task has not answered.
   */
  lastLine?: string;
}

export interface AgentHeadersView {
  sessionId: string;
  headers: AgentHeaderView[];
  lastOffset: string;
  eventsRead: string;
  objectsRead: string;
}

export interface RowHintsView {
  renderable: boolean;
  groupable: boolean;
  hasReasoning: boolean;
  durationMs: number;
  shortText: string;
  linesAdded: number;
  linesRemoved: number;
  status: string;
}

export type RowFacts =
  | { type: "user"; inputId: string; source: string; mode: string; provenance: string; untrusted: boolean }
  | { type: "stream"; streamId: string; phase: string; firstOffset: string; lastOffset: string; deltaCount: number; contentHash: string; abortSource: string; abortCode: string }
  | { type: "tool"; toolCallId: string; toolName: string; toolClass: string; effectClass: string; state: string; failureCode: string; paths: string[] }
  | { type: "approval"; kind: string; approvalId: string; toolCallId: string; toolName: string; effectClass: string; state: string; resolver: string }
  | { type: "group"; turnId: string; label: string; steps: number; reads: number; searches: number; commands: number; edits: number; others: number; linesAdded: number; linesRemoved: number; durationMs: number; open: boolean }
  | { type: "footer"; turnId: string; runId: string; outcome: string; failureCode: string; durationMs: number; inputTokens: string; outputTokens: string }
  | { type: "tail"; phase: string; label: string; detail: string }
  | { type: "boundary"; gapMs: number; day: string }
  | { type: "none" };

export interface TranscriptRowView {
  ordinal: number;
  rowId: string;
  kind: RowKind;
  offset: string;
  lastOffset: string;
  atMs: number;
  turnId: string;
  hints: RowHintsView;
  text: string;
  textTruncated: boolean;
  textRef: string;
  children: TranscriptRowView[];
  facts: RowFacts;
}

/** A task's facts the Core folded from its log at the page's offset (PX-196); "" where the log has none. */
export interface TaskFactsView {
  phase: string;
  agents: { total: number; running: number; background: number; waiting: number; done: number; failed: number };
  attachments: number;
  latestEvidence: string;
  risk: string;
  nextAction: string;
}

export interface TranscriptPageView {
  taskId: string;
  density: Density;
  rows: TranscriptRowView[];
  totalRows: number;
  nextAfterRow: number;
  hasMore: boolean;
  asOfOffset: string;
  lastOffset: string;
  taskState: string;
  readOffset: string;
  eventsRead: string;
  /** Absent from a Core older than PX-196. */
  facts?: TaskFactsView;
}

export interface TranscriptOptions {
  density?: Density;
  afterRow?: number;
  limit?: number;
  asOfOffset?: string;
}

export interface SnippetView {
  rowId: string;
  kind: RowKind;
  source: string;
  offset: string;
  turnId: string;
  text: string;
  cutBefore: boolean;
  cutAfter: boolean;
  matches: { start: number; end: number }[];
  score: number;
}

export interface ConversationHitView {
  taskId: string;
  title: string;
  statusClass: StatusClass;
  statusLabel: string;
  archived: boolean;
  titleMatched: boolean;
  matchedRows: number;
  score: number;
  lastOffset: string;
  snippets: SnippetView[];
}

export interface ConversationSearchView {
  sessionId: string;
  query: string;
  hits: ConversationHitView[];
  totalHits: number;
  hasMore: boolean;
  asOfOffset: string;
}

export interface SearchOptions {
  limit?: number;
  maxSnippets?: number;
  includeArchived?: boolean;
  taskId?: string;
}

/** The text of an assistant stream that is still open, as main has seen it (the transcript carries the text only once the stream completes). */
export interface OpenStreamView {
  /** The stream's id as 32 hex characters (a transcript row names it hyphenated). */
  streamId: string;
  taskId: string;
  /** The delta sequence of the last piece in `text`, from 1. */
  sequence: number;
  text: string;
}

/** A protected effect waiting for the person's decision, from the Core's approval list: the exact intent the decision must name. */
export interface PendingApprovalView {
  approvalId: string;
  taskId: string;
  toolCallId: string;
  toolName: string;
  effectClass: string;
  intentHash: string;
  requestedAtMs: number;
}

/** The states the Core's work graph gives a step (REQ-PX-200); the renderer shows them and never derives one. */
export const WORK_STATUSES = ["PENDING", "READY", "ACTIVE", "BLOCKED", "DONE", "FAILED", "CANCELLED"] as const;
export type WorkStatus = (typeof WORK_STATUSES)[number];

/** One step of a task's plan as the Core's work graph holds it. */
export interface WorkStepView {
  id: string;
  title: string;
  /** The Core's state for the step; a name this build does not know is passed as it came. */
  status: WorkStatus | string;
  dependsOn: string[];
  blockers: string[];
  evidenceRefs: number;
  attempts: number;
  /** An agent owns the step (a child task works it). */
  owned: boolean;
  planVersion: number;
}

/** The task's work graph (GetWorkGraph): the steps in the order they were made, and the latest plan version that changed one. */
export interface WorkGraphInfo {
  taskId: string;
  steps: WorkStepView[];
  ready: string[];
  planVersion: number;
}

/** The task's plan as recorded (GetPlan): the version in force with what it states, and how many versions there have been. */
export interface PlanInfo {
  taskId: string;
  /** 0 when the task has no plan. */
  currentVersion: number;
  outcome: string;
  /** model | user_review | cli */
  provenance: string;
  reason: string;
  versions: number;
  /** The steps the version in force itself names (a plan may name none: the work graph is then unchanged by it). */
  stepsInVersion: number;
  executedVersions: number[];
}
