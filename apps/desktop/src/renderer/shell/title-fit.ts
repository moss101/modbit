/**
 * Which pane toggles stand in the title bar (REQ-PX-202, after the owner's review: "Fix title bar first"), pure. The boards show
 * a few there: Details, Changes and Terminal, and a pane that is open or minimized. Every pane is in the Panes menu beside them.
 * The title has priority: a toggle leaves the bar for the menu before the title is cut, and the title is cut only once no
 * toggle is left to move.
 */
import { APP_KINDS, type AppKind } from "./prefs.ts";
import type { PaneState } from "./dock.ts";

/** The title is never narrower than this while a toggle could still move into the Panes menu instead. */
export const TITLE_FLOOR = 200;
/** One toggle's width in the bar and the gap after it (control-md 28, gap 2). */
export const TOGGLE_STEP = 30;
/** The panes that stand in the bar whatever their state, as on the boards. */
export const ALWAYS_SHOWN: readonly AppKind[] = ["changes", "terminal"];

/** How many toggles fit in `room` px (a group of n is n steps less one gap, plus the group's 8 px margin). */
export function fitCount(room: number, wanted: number): number {
  if (!Number.isFinite(room) || room <= 8) return 0;
  return Math.max(0, Math.min(wanted, Math.floor((room - 8 + 2) / TOGGLE_STEP)));
}

/**
 * The toggles that stand in the bar, in the panes' order: the ones the boards show (Changes, Terminal) and the ones open or
 * minimized, as many as `fit` allows; when not all fit, the open and minimized ones go before Changes and Terminal (they say
 * what is on screen), and among those the earlier pane goes first.
 */
export function shownToggles(panes: readonly { kind: AppKind; state: PaneState }[], fit: number): AppKind[] {
  const candidates = panes.filter((p) => ALWAYS_SHOWN.includes(p.kind) || p.state !== "closed");
  const rank = (p: { kind: AppKind; state: PaneState }) => (p.state === "open" ? 0 : p.state === "minimized" ? 1 : 2) * 100 + APP_KINDS.indexOf(p.kind);
  const kept = new Set([...candidates].sort((a, b) => rank(a) - rank(b)).slice(0, Math.max(0, fit)).map((p) => p.kind));
  return APP_KINDS.filter((k) => kept.has(k));
}

/** How many toggles the bar wants before any is moved to the menu. */
export const wantedToggles = (panes: readonly { kind: AppKind; state: PaneState }[]): number => panes.filter((p) => ALWAYS_SHOWN.includes(p.kind) || p.state !== "closed").length;
