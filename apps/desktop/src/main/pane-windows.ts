/**
 * Pane windows (REQ-PX-203; DR-PX-2026-11-11-021 MWS-10 and section 9), the decisions only, pure so the node tests cover them
 * without Electron. A pane popped out of the dock is a window of its own, made by the one window factory in main.ts with the
 * same preload, web preferences and content security policy as the app's window. What differs is what it may ask: the app's
 * window may ask every channel; a pane window may ask only the channels its pane needs, and only about the task it shows. No
 * channel that decides an approval, answers a question, changes a mode or a rule, or starts, cancels or reviews a task is on its
 * list, so an approval can never be decided from a pane window whatever its renderer sends (section 9: approvals stay with the
 * app's window and its docked card).
 */

/** The panes that can stand in a window of their own. Each is a view over typed reads of one task (PX-048 owns their content). */
export const POPPABLE = ["terminal", "changes", "files", "evidence"] as const;
export type PoppableKind = (typeof POPPABLE)[number];
export const isPoppable = (v: unknown): v is PoppableKind => typeof v === "string" && (POPPABLE as readonly string[]).includes(v);

/** The pane a window shows: one pane of one task of one session. */
export interface PaneRef {
  sessionId: string;
  taskId: string;
  kind: PoppableKind;
}

/**
 * What a pane was showing, handed between the dock and its window both ways so the pane comes back as it was: the terminal it
 * showed (the process itself is the Core's and never moves), the file chosen in Changes and how far its list was scrolled.
 * View choices only: no fact of the task is in it.
 */
export interface PaneViewState {
  terminalId?: string;
  path?: string;
  scrollTop?: number;
}

const HANDLE = /^[A-Za-z0-9_.:-]{1,128}$/;

/** A view state from a renderer, kept only where every field is what it says (anything else is dropped, never forwarded). */
export function cleanViewState(v: unknown): PaneViewState {
  if (!v || typeof v !== "object" || Array.isArray(v)) return {};
  const o = v as Record<string, unknown>;
  const out: PaneViewState = {};
  if (typeof o.terminalId === "string" && HANDLE.test(o.terminalId)) out.terminalId = o.terminalId;
  if (typeof o.path === "string" && o.path.length > 0 && o.path.length <= 4096 && !o.path.includes("\0") && !o.path.startsWith("/") && !/^[A-Za-z]:/.test(o.path) && !o.path.split(/[\\/]/).includes("..")) out.path = o.path;
  if (typeof o.scrollTop === "number" && Number.isFinite(o.scrollTop) && o.scrollTop >= 0 && o.scrollTop <= 10_000_000) out.scrollTop = Math.round(o.scrollTop);
  return out;
}

/**
 * The channels a pane window may ask, and where in each the task (and the session) it names stands: a call that names another
 * task or session is refused. `null` for a channel whose argument is a handle main already ties to the window that opened it
 * (a terminal attachment), or that names no task.
 */
interface PaneChannel {
  /** The panes that need the channel. */
  kinds: readonly PoppableKind[];
  task: number | null;
  session: number | null;
}
const T: readonly PoppableKind[] = ["terminal"];
const C: readonly PoppableKind[] = ["changes"];
export const PANE_CHANNELS: Readonly<Record<string, PaneChannel>> = {
  // The window's own: who it is, what it shows, asking the app's window to show something, going back to the dock.
  "pane:context": { kinds: POPPABLE, task: null, session: null },
  "pane:state": { kinds: POPPABLE, task: null, session: null },
  "pane:show": { kinds: POPPABLE, task: null, session: null },
  "pane:return": { kinds: POPPABLE, task: null, session: null },
  "core:status": { kinds: POPPABLE, task: null, session: null },
  // Terminal (REQ-PX-048): the task's terminals, attached by cursor; the attachment handles are the window's own (terminal-host.ts).
  "terminal:list": { kinds: T, task: 0, session: null },
  "terminal:attach": { kinds: T, task: 1, session: 0 },
  "terminal:ack": { kinds: T, task: null, session: null },
  "terminal:detach": { kinds: T, task: null, session: null },
  "terminal:input": { kinds: T, task: null, session: null },
  "terminal:resize": { kinds: T, task: null, session: null },
  "terminal:write": { kinds: T, task: null, session: null },
  "terminal:kill": { kinds: T, task: 1, session: 0 },
  "terminal:history": { kinds: T, task: 0, session: null },
  // Changes (REQ-PX-048, PX-104): the diff, the index and the actions on them; Commit and push asks the Core, which answers with
  // the approval the app's window decides. The undo of a revert is the checkpoint the revert took.
  "review:bundle": { kinds: [...C, "evidence"], task: 0, session: null },
  "review:codeView": { kinds: C, task: 0, session: null },
  "changes:state": { kinds: C, task: 0, session: null },
  "changes:revert": { kinds: C, task: 1, session: 0 },
  "changes:decideHunk": { kinds: C, task: 1, session: 0 },
  "changes:stage": { kinds: C, task: 1, session: 0 },
  "changes:commitPush": { kinds: C, task: 1, session: 0 },
  "checkpoints:restore": { kinds: C, task: 1, session: 0 },
  // Files and Evidence: reads.
  "files:list": { kinds: ["files"], task: 0, session: null },
  "files:read": { kinds: ["files"], task: 0, session: null },
  "evidence:receipts": { kinds: ["evidence"], task: 0, session: null },
  "context:inspector": { kinds: ["evidence"], task: 0, session: null },
};

/** Why a pane window may not ask `channel` with `args`, or null when it may. */
export function paneRefusal(pane: PaneRef, channel: string, args: readonly unknown[]): string | null {
  const c = PANE_CHANNELS[channel];
  if (!c) return "a pane window may not ask this";
  if (!c.kinds.includes(pane.kind)) return `the ${pane.kind} pane does not ask this`;
  if (c.task !== null && args[c.task] !== pane.taskId) return "a pane window asks only about its own task";
  if (c.session !== null && args[c.session] !== pane.sessionId) return "a pane window asks only about its own session";
  return null;
}

/** What `pane:show` may ask the app's window to show: the task's review, a call in its conversation, or the waiting approval. */
export type PaneShowRequest = { what: "review" } | { what: "approval" } | { what: "call"; toolCallId: string };
export function cleanShowRequest(v: unknown): PaneShowRequest | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  if (o.what === "review" || o.what === "approval") return { what: o.what };
  if (o.what === "call" && typeof o.toolCallId === "string" && HANDLE.test(o.toolCallId)) return { what: "call", toolCallId: o.toolCallId };
  return null;
}

/** The size a pane window opens at: the pane's own size in the dock where it is known, within what a pane may be and the screen holds. */
export const PANE_WINDOW_MIN = { width: 320, height: 240 } as const;
export const PANE_WINDOW_DEFAULT = { width: 720, height: 560 } as const;
/** A pane window opens at least this big, even from a narrow place in the dock; the person can make it smaller. */
export const PANE_WINDOW_OPEN_MIN = { width: 560, height: 420 } as const;

/** A short, plain name for a pane window: "Terminal: <task title>". */
export const PANE_NAME: Record<PoppableKind, string> = { terminal: "Terminal", changes: "Changes", files: "Files", evidence: "Evidence" };
export function paneWindowTitle(kind: PoppableKind, taskTitle: string): string {
  const t = taskTitle.replace(/\s+/g, " ").trim().slice(0, 120);
  return t ? `${PANE_NAME[kind]}: ${t}` : PANE_NAME[kind];
}
