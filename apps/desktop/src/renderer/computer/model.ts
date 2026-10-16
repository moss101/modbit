/**
 * The view model of native computer control's desktop surface (PX-074; docs/66
 * CUC-E05, CUC-H01, CUC-D05): pure functions over the Core's views. The words
 * are Modbit's own. Nothing here decides who holds the machine, whether a
 * permission is granted or whether a stop is sticky: the Core said, and this
 * file chooses how to say it. Every state carries words as well as a glyph and
 * a colour; nothing is conveyed by colour alone.
 */
import type { ComputerCardView, ComputerPresenceEntry, ComputerPresenceInfo, PermissionName, PermissionState, PresenceState } from "../../shared/computer-types.ts";

// ------------------------------------------------------- the permission window

/** One row of the first-run window: a minimum capability in plain language, never an operating-system term alone. */
export interface PermissionDef {
  name: PermissionName;
  /** What Modbit asks for, in plain words (CUC-E05). */
  label: string;
  /** What it is used for. */
  why: string;
  /** The one-time steps in the operating system's settings. */
  steps: string;
  /** The operating system's own name for the pane, so the person can find it. */
  pane: string;
}

export const PERMISSION_DEFS: readonly PermissionDef[] = [
  {
    name: "accessibility",
    label: "Access to application interfaces",
    why: "Lets the helper read an application's buttons and fields and press them, so the agent can work in another application without taking your mouse.",
    steps: "In System Settings, open Privacy & Security, then Accessibility, and turn on the Modbit helper.",
    pane: "Accessibility",
  },
  {
    name: "screen_recording",
    label: "Use of screenshots as context",
    why: "Lets the helper take a picture of the one window the agent is working in, for the cases where reading its interface is not enough. Secure fields are blacked out before a picture is kept.",
    steps: "In System Settings, open Privacy & Security, then Screen Recording, and turn on the Modbit helper.",
    pane: "Screen Recording",
  },
];

/** `checking` (the helper has not said), `allow` (the person has a step to take), `ready` (granted). */
export type RowStatus = "checking" | "allow" | "ready";

export function rowStatus(state: PermissionState, attached: boolean): RowStatus {
  if (!attached) return "checking";
  return state === "GRANTED" ? "ready" : state === "DENIED" ? "allow" : "checking";
}

export const ROW_WORDS: Record<RowStatus, { label: string; detail: string }> = {
  checking: { label: "Checking", detail: "Waiting for the helper to say whether this is on." },
  allow: { label: "Allow", detail: "Not on yet. Turn it on in System Settings, then come back." },
  ready: { label: "Ready", detail: "On. Nothing more to do." },
};

export function permissionStateOf(p: ComputerPresenceInfo, name: PermissionName): PermissionState {
  return name === "accessibility" ? p.accessibility : p.screenRecording;
}

/** The helper is attached and at least one permission it needs is off. */
export function permissionsNeeded(p: ComputerPresenceInfo | null): boolean {
  return !!p && p.attached && (p.accessibility === "DENIED" || p.screenRecording === "DENIED");
}

export function allReady(p: ComputerPresenceInfo | null): boolean {
  return !!p && p.attached && p.accessibility === "GRANTED" && p.screenRecording === "GRANTED";
}

/** A line for the window when there is no helper to ask. */
export function noHelperWords(p: ComputerPresenceInfo | null): string {
  if (!p) return "Waiting for the Core.";
  if (!p.configured) return "This build has no computer-control helper configured, so the agent cannot operate other applications.";
  return p.detail ? `The computer-control helper is not running: ${p.detail}` : "The computer-control helper is not running yet.";
}

// ------------------------------------------------------------------- presence

export const STATE_WORDS: Record<PresenceState, { label: string; detail: string }> = {
  CONTROLLING: { label: "Agent in control", detail: "The agent is operating your computer." },
  PARKED: { label: "Paused: you are using the computer", detail: "You touched the keyboard or pointer. The agent waits until you stop." },
  STOPPED: { label: "Stopped", detail: "Control was stopped." },
  USER_ABORTED: { label: "Stopped by you", detail: "You pressed Stop. The agent cannot use the computer again until the next turn." },
  LATCHED: { label: "Outcome unknown", detail: "An input may have happened and Modbit cannot tell. Nothing more is sent until you look and say so." },
};

/** The agent holds, or is paused on, the machine now: a Stop control is shown. */
export const holdsMachine = (e: ComputerPresenceEntry): boolean => e.state === "CONTROLLING" || e.state === "PARKED";

export function scopeWords(scope: string): string {
  return scope === "SCREEN" ? "the whole screen and your mouse" : scope === "APP" ? "in the background, without moving your mouse" : "";
}

/** What a screen reader hears when control is acquired, paused, stopped or released (CUC-H01). */
export function announce(prev: readonly ComputerPresenceEntry[], next: readonly ComputerPresenceEntry[], titleOf: (taskId: string) => string): string {
  const key = (e: ComputerPresenceEntry): string => e.taskId;
  const before = new Map(prev.map((e) => [key(e), e]));
  const lines: string[] = [];
  for (const e of next) {
    const was = before.get(key(e));
    const task = titleOf(e.taskId);
    if (e.state === "USER_ABORTED" && was?.state !== "USER_ABORTED") lines.push(`Computer control stopped by you for “${task}”. It cannot resume this turn.`);
    else if (e.state === "LATCHED" && was?.state !== "LATCHED") lines.push(`Computer control for “${task}” has an unknown outcome and is paused.`);
    else if (holdsMachine(e) && !(was && holdsMachine(was))) lines.push(`Computer control acquired: the agent “${task}” is operating ${e.application || "an application"}.`);
    else if (e.state === "PARKED" && was?.state === "CONTROLLING") lines.push(`Computer control paused: you are using the computer.`);
  }
  for (const w of prev) {
    if (holdsMachine(w) && !next.some((e) => key(e) === key(w) && holdsMachine(e)) && !next.some((e) => key(e) === key(w) && (e.state === "USER_ABORTED" || e.state === "LATCHED"))) lines.push(`Computer control released by “${titleOf(w.taskId)}”.`);
  }
  return lines.join(" ");
}

/** Entries the bar shows: the ones that hold the machine, and the sticky or unresolved ones that still need to be read. */
export const barEntries = (p: ComputerPresenceInfo | null): ComputerPresenceEntry[] => (p ? p.controls : []);

// ------------------------------------------------------------------ the cards

/** The Core's typed refusals in plain words, with what the person can do (the model reads its own recovery text; this is the person's). */
export const REFUSAL_WORDS: Record<string, { headline: string; next: string }> = {
  TARGET_STALE: { headline: "The element had changed", next: "Nothing was done. The agent looks again before it tries." },
  TARGET_OCCLUDED: { headline: "Something covered the target", next: "Nothing was done. The agent looks again before it tries." },
  WINDOW_UNVERIFIABLE: { headline: "The window was not the approved one, or cannot be driven", next: "Nothing was done. Some applications can never be driven, such as system settings, terminals and password stores." },
  ACCESSIBILITY_UNAVAILABLE: { headline: "The application shows no interface to read", next: "Nothing was done. The agent can use a screenshot instead." },
  HUMAN_ACTIVE: { headline: "You were using the computer", next: "Nothing was done. The agent waits until you stop." },
  MODAL_BLOCKING: { headline: "A dialog is open in the way", next: "Nothing was done. The dialog is yours to answer." },
  TARGET_NOT_EDITABLE: { headline: "That field does not take that input", next: "Nothing was done." },
  ACTION_UNSAFE: { headline: "The action is protected", next: "Nothing was done. Modbit does not act around this." },
  PERMISSION_REQUIRED: { headline: "A permission is off", next: "Nothing was done. Turn the permission on in System Settings." },
  SECURE_DESKTOP: { headline: "The screen is locked or showing a secure prompt", next: "Nothing was captured or done." },
  TARGET_ELEVATED: { headline: "The application runs with more privilege than the helper", next: "Nothing was done." },
  CAPTURE_FAILED: { headline: "The screenshot failed", next: "Nothing was done. The agent can try again." },
  SCREENSHOT_REQUIRED: { headline: "A fresh screenshot was needed first", next: "Nothing was done." },
  INVALID_ARGUMENTS: { headline: "The request was malformed", next: "Nothing was done." },
  INPUT_FAILED: { headline: "The input did not go through", next: "Provably nothing was injected." },
  TIMEOUT: { headline: "It took too long", next: "Nothing further was sent." },
  UNSUPPORTED_REQUEST: { headline: "The helper cannot do that", next: "Nothing was done." },
  ACTUATOR_UNAVAILABLE: { headline: "No helper is running", next: "Nothing was done." },
  OUTCOME_UNKNOWN: { headline: "An earlier input may have happened", next: "Nothing more is sent until you look at the machine and say so, or the agent observes it afresh." },
  SESSION_REQUIRED: { headline: "No control session was open", next: "Nothing was done." },
  SESSION_BUSY: { headline: "Another task holds the machine", next: "Nothing was done." },
  USER_ABORTED: { headline: "You stopped it", next: "Control stays stopped until the next turn." },
  APPROVAL_DENIED: { headline: "You skipped this action", next: "Nothing was done." },
  GRANT_EXPIRED: { headline: "The observation grant ended", next: "Nothing was done. The agent asks again for a new one." },
};

export function refusalWords(code: string): { headline: string; next: string } {
  return REFUSAL_WORDS[code] ?? { headline: `Refused (${code.replace(/_/g, " ").toLowerCase()})`, next: "Nothing was done." };
}

const TOOL_TITLES: Record<string, string> = {
  "computer.start": "Start control",
  "computer.start_screen": "Start control of the screen",
  "computer.release": "Give control back",
  "computer.apps": "List applications",
  "computer.resolve": "Look up an application",
  "computer.state": "Read the application's interface",
  "computer.screenshot": "Take a screenshot",
  "computer.wait": "Wait",
  "computer.press": "Press",
  "computer.set_value": "Set a value",
  "computer.click": "Click",
  "computer.move": "Move the pointer",
  "computer.drag": "Drag",
  "computer.type": "Type",
  "computer.key": "Press keys",
  "computer.scroll": "Scroll",
};

export const toolTitle = (tool: string): string => TOOL_TITLES[tool] ?? tool.replace(/^computer\./, "").replace(/_/g, " ");

export type CardTone = "ok" | "warn" | "danger" | "info" | "running" | "idle";

export interface CardStatus {
  tone: CardTone;
  label: string;
  /** One line of what happened and what happens next; "" when there is nothing to add. */
  detail: string;
}

/** The status of a computer tool call, in the Core's typed words: done, refused, unknown outcome, stopped. */
export function cardStatus(card: ComputerCardView): CardStatus {
  if (card.latched) return { tone: "warn", label: "Outcome unknown", detail: REFUSAL_WORDS.OUTCOME_UNKNOWN!.next };
  if (card.state === "UNKNOWN_OUTCOME" || card.outcome === "UNKNOWN") return { tone: "warn", label: "Outcome unknown", detail: "The input may have happened and nothing says whether it did." };
  if (card.userAborted) return { tone: "danger", label: "Stopped by you (USER_ABORTED)", detail: refusalWords("USER_ABORTED").next };
  const code = card.failureCode || (card.outcome === "FAILED" ? "INPUT_FAILED" : "");
  if (card.state === "DENIED" || code === "APPROVAL_DENIED") return { tone: "idle", label: "Skipped", detail: "You skipped this action. Nothing was done." };
  if (code || card.state === "FAILED") {
    const w = refusalWords(code || "INPUT_FAILED");
    return { tone: "danger", label: `Refused: ${code ? code.replace(/_/g, " ").toLowerCase() : "failed"}`, detail: `${w.headline}. ${w.next}` };
  }
  if (card.state === "AWAITING_APPROVAL") return { tone: "warn", label: "Waiting for your approval", detail: "" };
  if (card.state === "RUNNING" || card.state === "PENDING") return { tone: "running", label: "Running", detail: "" };
  if (card.state === "CANCELLED") return { tone: "idle", label: "Cancelled", detail: "" };
  return { tone: "ok", label: "Done", detail: card.outcome === "SUCCEEDED" && card.verified === false ? "The expected change was not seen afterwards." : "" };
}

export function modalityWords(m: string): string {
  return m === "ax" ? "through the application's interface" : m === "coordinate" ? "at screen coordinates" : m === "raw" ? "as raw input" : "";
}

/** Where an input lands, in words: the element the approval named. */
export function targetWords(card: ComputerCardView): string {
  const t = card.target;
  if (!t) return "";
  if (t.role || t.name) return `${t.role || "element"} “${t.name}”`;
  return t.id;
}

/** Numbers the computer calls of a transcript in order, 1-based. */
export function ordinals(toolCallIds: readonly string[]): Map<string, number> {
  return new Map(toolCallIds.map((id, i) => [id, i + 1]));
}

/** A tool call id as the transcript and the Core's records both name it: hex without dashes. */
export const callKey = (id: string): string => id.replace(/-/g, "").toLowerCase();

// ------------------------------------------------------------- the approval card

/** No computer approval offers "always": this is the whole rule, and the card renders no allowlist affordance at all. */
export const alwaysOffered = (_toolName: string): false => false;

export function approvalTitle(tool: string, kind: "action" | "observation_grant"): string {
  if (kind === "observation_grant") return tool === "computer.start_screen" ? "Take over the screen and mouse" : "Let the agent observe one application";
  return `${toolTitle(tool)} in another application`;
}
