/**
 * The composer's plain data (REQ-PX-054, -055, -056): what main hands the
 * renderer for the task mode, the durable input queue, the send behaviour, the
 * slash inventory, the model catalog, an ingested attachment and a side
 * answer. Every shape is a view of a Core fact; the renderer keeps no copy
 * that outlives a read and decides nothing a Core command decides (docs/81).
 */

/** The task's modes, in the Core's names (tasking.proto `TaskMode`). */
export const TASK_MODES = ["AGENT", "PLAN", "DEBUG", "MULTITASK", "ASK"] as const;
export type TaskModeId = (typeof TASK_MODES)[number];

/** The objective profiles the Core's router understands (`ObjectiveProfile`). */
export const OBJECTIVES = ["COST", "BALANCE", "INTELLIGENCE"] as const;
export type ObjectiveId = (typeof OBJECTIVES)[number];

/** How a message is queued: DEFAULT is plain Enter, which the Core resolves from the task's send behaviour. */
export const INPUT_MODES = ["DEFAULT", "STEER", "COLLECT", "FOLLOW_UP"] as const;
export type InputModeId = (typeof INPUT_MODES)[number];

export interface PostureView {
  /** The latest mode the person set (the Core's field). */
  mode: TaskModeId;
  /** The mode the Kernel enforces right now; differs from `mode` until the next round boundary. */
  modeInForce: TaskModeId;
  modeOffset: string;
  writes: boolean;
  effectCeiling: string;
  subagents: boolean;
  reproductionFirst: boolean;
  preference: {
    objective: string;
    effort: string;
    serviceTier: string;
    pinEndpoint: string;
    pinModel: string;
    offset: string;
    appliedOffset: string;
    effortApplied: string;
    serviceTierApplied: string;
  };
  routing: { outcome: string; reasonCode: string; detail: string; floorMode: string };
}

export interface ModeChangedView {
  mode: TaskModeId;
  previousMode: TaskModeId;
  /** IMMEDIATE (no run in flight) or NEXT_ROUND_BOUNDARY. */
  effective: string;
  offset: string;
}

export interface PreferencePatch {
  objective?: ObjectiveId;
  effort?: string;
  serviceTier?: string;
  pin?: { endpoint: string; model: string };
  clearPin?: boolean;
}

export interface PreferenceSetView {
  effective: string;
  offset: string;
  routing: { outcome: string; reasonCode: string; detail: string; floorMode: string };
  preference: PostureView["preference"];
}

export interface QueuedInputItem {
  inputId: string;
  position: number;
  /** STEER | COLLECT | FOLLOW_UP */
  mode: string;
  text: string;
  /** QUEUED | DISPATCHED | REMOVED */
  state: string;
  model: string;
  edited: boolean;
  sentNow: boolean;
  provenance: string;
  untrusted: boolean;
}

export interface QueueView {
  items: QueuedInputItem[];
  queued: number;
  /** A loop is running the task: queued inputs drain at its boundaries. */
  runAlive: boolean;
  offset: string;
}

export interface SendBehaviorState {
  whileRunning: string;
  sendNow: string;
  whileRunningValues: string[];
  sendNowValues: string[];
  /** The Core's own words for what each setting does (labels must state the consequence, AFW-E05). */
  whileRunningConsequence: string;
  sendNowConsequence: string;
}

export interface InterruptView {
  kind: string;
  runAlive: boolean;
  interruptId: string;
  offset: string;
}

export interface SlashEntryView {
  /** SKILL | COMMAND | SUBAGENT */
  kind: string;
  id: string;
  displayName: string;
  /** Untrusted text from a skill's author: shown as text, never as markup. */
  description: string;
  scope: string;
  trust: string;
  trustDetail: string;
  enabled: boolean;
  invocation: string;
  builtIn: boolean;
}

export interface SlashInventoryView {
  entries: SlashEntryView[];
  /** Index of the first entry after the divider; 0 = none. */
  dividerAt: number;
}

export interface ModelVariantView {
  effort: string;
  serviceTier: string;
  label: string;
  isDefault: boolean;
  raisesCost: boolean;
}

export interface ModelEntryView {
  endpoint: string;
  model: string;
  provider: string;
  reasoning: boolean;
  vision: boolean;
  contextTokens: number;
  defaultEffort: string;
  defaultServiceTier: string;
  variants: ModelVariantView[];
  credentialAvailable: boolean;
  blockedByPolicy: string;
  pinRefusalCode: string;
  pinAllowed: boolean;
}

export interface ModelCatalogView {
  models: ModelEntryView[];
  objectives: string[];
  defaultObjective: string;
}

export interface AttachmentResult {
  attachmentId: string;
  kind: string;
  mime: string;
  contentRef: string;
  replayed: boolean;
  /** The file's name as given (a label only). */
  name: string;
  bytes: number;
  /** A small data: image made by main for an image the Core accepted; null otherwise. */
  thumbnail: string | null;
}

export interface SideAnswerView {
  text: string;
  inputTokens: string;
  outputTokens: string;
  snapshotMessages: number;
  lastOffset: string;
}

export interface QueuedChangeView {
  inputId: string;
  state: string;
  position: number;
  offset: string;
}
