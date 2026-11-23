/**
 * Split view (DR-PX-2026-11-11-021 MWS-11, REQ-PX-204), pure: two tasks side by side in the centre, each with its own title
 * bar, transcript, composer and approval card; one of them has the keyboard. At most two: a third is refused with its reason,
 * never accepted. A view choice of this window, stored with the layout preferences so a reload finds the same two tasks, the
 * same focus and the same ratio; the Core knows nothing of it.
 */
import { clampSplitRatio, SPLIT_LAYOUT, SPLIT_NEED } from "./layout.ts";

export { clampSplitRatio, splitColumns, SPLIT_LAYOUT, SPLIT_NEED } from "./layout.ts";

export interface SplitState {
  /** The two tasks, left then right. Never the same task twice, never more than two. */
  tasks: [string, string];
  /** The left column's share of the two columns' width. */
  ratio: number;
  /** Which column has the keyboard: 0 is the left one. */
  focus: 0 | 1;
}

export const MAX_SPLIT_TASKS = 2;

/**
 * Validates a stored split. Null where it is not one: not two distinct task ids. A stored list of three or more is not
 * trimmed to two: a store that says three is not a split this view makes, and it reads as none.
 */
export function parseSplit(v: unknown): SplitState | null {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return null;
  const s = v as Record<string, unknown>;
  if (!Array.isArray(s.tasks) || s.tasks.length !== MAX_SPLIT_TASKS) return null;
  const [a, b] = s.tasks as unknown[];
  if (typeof a !== "string" || typeof b !== "string" || a.length === 0 || b.length === 0 || a.length > 200 || b.length > 200 || a === b) return null;
  return { tasks: [a, b], ratio: clampSplitRatio(s.ratio), focus: s.focus === 1 ? 1 : 0 };
}

export type SplitRequest = { ok: true; split: SplitState } | { ok: false; reason: string };

/**
 * What asking for `taskId` in split view does, beside the task the centre shows (`shown`, null on home or a page) and the split
 * already open. Refused, with the reason in words: nothing in the centre to sit beside; the task is already there; a third
 * task. Accepted: the shown task on the left, the new one on the right with the keyboard.
 */
export function requestSplit(current: SplitState | null, shown: string | null, taskId: string, ratio: number = SPLIT_LAYOUT.ratioDefault): SplitRequest {
  if (current) {
    if (current.tasks.includes(taskId)) return { ok: false, reason: "That task is already in split view." };
    return { ok: false, reason: "Split view holds two tasks. Close one of them first: Close in its title bar." };
  }
  if (shown === null) return { ok: false, reason: "Open a task first: split view puts this one beside it." };
  if (shown === taskId) return { ok: false, reason: "That task is already open." };
  return { ok: true, split: { tasks: [shown, taskId], ratio: clampSplitRatio(ratio), focus: 1 } };
}

/** The task left when one column closes; split view is over. */
export function remainingAfterClose(split: SplitState, column: 0 | 1): string {
  return split.tasks[column === 0 ? 1 : 0];
}

/**
 * A plain open (a click on a row, the palette, a link in a transcript) while two tasks are side by side: a task already in a
 * column takes the keyboard there; another task takes the focused column's place. Split view stays at two.
 */
export function openInSplit(split: SplitState, taskId: string): SplitState {
  const at = split.tasks.indexOf(taskId);
  if (at === 0 || at === 1) return at === split.focus ? split : { ...split, focus: at };
  const tasks: [string, string] = split.focus === 0 ? [taskId, split.tasks[1]] : [split.tasks[0], taskId];
  return { ...split, tasks };
}

/** The focused column's task. */
export const focusedTask = (split: SplitState): string => split.tasks[split.focus];

/** The ratio that puts the divider `leftPx` from the left edge of a centre `centerPx` wide (what a drag or a key stores). */
export function ratioForColumns(leftPx: number, centerPx: number): number {
  return centerPx > 0 ? clampSplitRatio(leftPx / centerPx) : SPLIT_LAYOUT.ratioDefault;
}

/** Why split view cannot show its two tasks in a task area `avail` wide, in words; empty when it can. */
export function splitRefusal(avail: number): string {
  return avail >= SPLIT_NEED ? "" : `Split view needs ${SPLIT_NEED} px beside the task list for two tasks; this window gives ${Math.max(0, Math.round(avail))}. Widen the window or hide the task list.`;
}
