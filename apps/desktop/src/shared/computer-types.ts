/**
 * The plain, structured-clone-safe shapes native computer control's desktop
 * surface crosses the preload bridge as (PX-074; docs/66 CUC-D05, CUC-E05,
 * CUC-H01). Main converts the Core's messages to these; the renderer renders
 * them and keeps no state of its own about control: who holds the machine,
 * which permissions are granted, whether a Stop is sticky and whether a latch
 * stands are the Core's answers. Every string an application or the model
 * produced (a window title, an element name, a reason) is an untrusted label:
 * it is shown quoted, bounded, and never read as an instruction.
 */

/** What the Core knows of an operating-system permission: only what the actuator said. */
export type PermissionState = "GRANTED" | "DENIED" | "UNKNOWN";

export const PERMISSIONS = ["accessibility", "screen_recording"] as const;
export type PermissionName = (typeof PERMISSIONS)[number];

/** The state of one control (a task that holds, or has just lost, the machine). */
export const PRESENCE_STATES = ["CONTROLLING", "PARKED", "STOPPED", "USER_ABORTED", "LATCHED"] as const;
export type PresenceState = (typeof PRESENCE_STATES)[number];

export interface ComputerPresenceEntry {
  taskId: string;
  /** "" when no session is open (a stop or a latch outlives the session). */
  controlId: string;
  /** The application's display name: an untrusted label. */
  application: string;
  bundleId: string;
  scope: "APP" | "SCREEN" | "";
  state: PresenceState;
  grantExpiresMs: number;
  /** "<tool> <action>: <reason>" while an unknown-outcome latch stands, else "". */
  latch: string;
  latchToolCallId: string;
  actions: number;
  screenshots: number;
  observations: number;
}

export interface ComputerPresenceInfo {
  configured: boolean;
  attached: boolean;
  actuatorName: string;
  actuatorVersion: string;
  platform: string;
  /** Why no actuator is attached, when none is. */
  detail: string;
  accessibility: PermissionState;
  screenRecording: PermissionState;
  controls: ComputerPresenceEntry[];
}

export interface ComputerStopInfo {
  sessionsClosed: number;
  actuatorStopped: boolean;
}

export interface OpenSettingsInfo {
  /** The pane the deep link names. */
  pane: PermissionName;
  /** The link was handed to the operating system (or, under the test switch, recorded). */
  opened: boolean;
  /** Why not, in words for the person. */
  note: string;
}

/** The exact intent of a native action or observation grant, read from the approval's scope (CUC-C01). */
export interface ComputerIntentView {
  /** The tool the approval is for (`computer.press`). */
  tool: string;
  /** `action` (one input) or `observation_grant` (the session's observation of one application). */
  kind: "action" | "observation_grant";
  /** `APP` or `SCREEN`. */
  scope: string;
  /** The application's identity as the operating system vouches for it: never the title. */
  application: { bundleId: string; executablePath: string; signingIdentity: string; pid: number };
  /** The display name: an untrusted label. */
  applicationName: string;
  window: { id: string; title: string };
  /** What the input lands on; null for a grant. */
  target: { kind: "element" | "coordinate" | "focus_or_keys"; snapshot: string; id: string; role: string; name: string; frame: string; windowVersion: number } | null;
  /** `press` | `set_value` | `click` | ... ; "" for a grant. */
  action: string;
  /** `ax` | `coordinate` | `raw`. */
  modality: string;
  /** The arguments in full (typed text is shown, not hidden), bounded. */
  argumentsText: string;
  argumentsTruncated: boolean;
  /** The agent's stated reason for a grant: an untrusted quote. */
  reason: string;
  takesDisplayAndCursor: boolean;
  /** The grant's bounds, for an observation grant. */
  grant: { endsWith: string; maxSeconds: number; callCap: number } | null;
  reversible: boolean;
  /** The newest masked screenshot the task's activity names ("" when none): the card's before-thumbnail. */
  beforeArtifact: string;
}

/** One computer tool call as an action card shows it, joined by main from the transcript, the activity and the approvals. */
export interface ComputerCardView {
  toolCallId: string;
  tool: string;
  /** The tool call's lifecycle state (the transcript's). */
  state: string;
  /** The typed refusal code, "" when none. */
  failureCode: string;
  /** What the tool call was: `start` | `observe` | `act` | `release` | `other`. */
  role: "start" | "observe" | "act" | "release" | "other";
  application: string;
  bundleId: string;
  scope: string;
  /** The element or target, when the call named one. */
  target: { role: string; name: string; id: string } | null;
  action: string;
  argumentsText: string;
  modality: string;
  /** `SUCCEEDED` | `FAILED` | `UNKNOWN` | "" (no action event). */
  outcome: string;
  verified: boolean | null;
  durationMs: number;
  /** The approval that bound this call. */
  approval: { approvalId: string; intentHash: string; status: string } | null;
  /** The observation the call made. */
  observation: { kind: string; elements: number; secureWithheld: number; maskedRegions: number; artifactRef: string; artifactExpired: boolean } | null;
  /** The unknown-outcome latch stands on this very call. */
  latched: boolean;
  /** The person's Stop ended this call's run. */
  userAborted: boolean;
}

export interface ComputerCardsInfo {
  taskId: string;
  cards: ComputerCardView[];
}

/** A masked frame as the card shows it: a data URL main made from the stored artifact (CUC-F01). */
export interface ComputerFrameInfo {
  dataUrl: string;
  width: number;
  height: number;
  /** The thumbnail is a reduction of the stored frame; the stored one is `full`. */
  full: boolean;
}
