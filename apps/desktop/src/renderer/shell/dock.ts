/**
 * The pane dock's model (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10 and section 5.1), pure: which of a task's panes are open, which
 * are minimized, how they tile and what a drag stores. Any of the task's panes can be open together; they tile in a grid of at
 * most two columns; a pane is never under 280 by 100 px. The state is a view choice kept in the layout preferences per task
 * (`PanelState` in prefs.ts); nothing here reads or holds a fact of the task.
 */
import { DOCK_SPLIT_DEFAULT, DOCK_SPLIT_MAX, DOCK_SPLIT_MIN, PANE_WEIGHT_MAX, PANE_WEIGHT_MIN, type AppKind, type PanelState } from "./prefs.ts";
import { clamp } from "./layout.ts";

/** A pane is never smaller than this (section 5.1). */
export const PANE_MIN = { width: 280, height: 100 } as const;
/** The grid has at most this many columns (MWS-10). */
export const DOCK_MAX_COLUMNS = 2;
/** The hairline between two columns (the panes' 1 px edge). */
export const DOCK_GAP = 1;

export type PaneState = "closed" | "open" | "minimized";

export function paneStateOf(s: PanelState, kind: AppKind): PaneState {
  if (!s.panes.includes(kind)) return "closed";
  return s.minimized.includes(kind) ? "minimized" : "open";
}

/** The panes in the grid, in order: the open ones the task can show now that are not minimized. */
export function tiledOf(s: PanelState, available: readonly AppKind[]): AppKind[] {
  return s.panes.filter((k) => available.includes(k) && !s.minimized.includes(k));
}

const without = (list: readonly AppKind[], kind: AppKind): AppKind[] => list.filter((k) => k !== kind);

/** Opens a pane, or brings a minimized one back to its place. An open pane stays where it is. */
export function openPane(s: PanelState, kind: AppKind): PanelState {
  if (!s.panes.includes(kind)) return { ...s, panes: [...s.panes, kind], minimized: without(s.minimized, kind) };
  return s.minimized.includes(kind) ? { ...s, minimized: without(s.minimized, kind) } : s;
}

/** Closes a pane: it leaves the set and its size is forgotten. The last set that held a pane is what the dock's toggle brings back. */
export function closePane(s: PanelState, kind: AppKind): PanelState {
  if (!s.panes.includes(kind)) return s;
  const weights = { ...s.weights };
  delete weights[kind];
  return { ...s, panes: without(s.panes, kind), minimized: without(s.minimized, kind), last: s.panes.filter((k) => !s.minimized.includes(k)).length > 0 ? s.panes.filter((k) => !s.minimized.includes(k)) : s.last, weights };
}

/** Takes an open pane out of the grid. It stays open: its place, its size and whatever it showed are kept for when it comes back. */
export function minimizePane(s: PanelState, kind: AppKind): PanelState {
  if (!s.panes.includes(kind) || s.minimized.includes(kind)) return s;
  return { ...s, minimized: [...s.minimized, kind] };
}

/** The title bar's toggle for one pane: a closed pane opens, a minimized one comes back, an open one closes. */
export function togglePane(s: PanelState, kind: AppKind): PanelState {
  return paneStateOf(s, kind) === "open" ? closePane(s, kind) : openPane(s, kind);
}

/**
 * The dock's own toggle (the keyboard's "toggle the panel", and the details rail asking for its place): with a pane in the grid
 * every pane closes and the set is remembered; with none, the remembered set comes back, or `fallback` where there is none.
 */
export function toggleDock(s: PanelState, available: readonly AppKind[], fallback: AppKind | undefined): PanelState {
  const tiled = tiledOf(s, available);
  if (tiled.length > 0) return { ...s, panes: [], minimized: [], last: s.panes.filter((k) => !s.minimized.includes(k)) };
  const back = s.last.filter((k) => available.includes(k));
  const panes = back.length > 0 ? back : fallback ? [fallback] : [];
  return { ...s, panes: [...new Set([...s.panes, ...panes])], minimized: s.minimized.filter((k) => !panes.includes(k)) };
}

/**
 * How the tiled panes sit in a dock `width` px wide: one column, or two where there are two panes or more and each column can be
 * 280 px. Two columns fill top to bottom, the left one first, and the left one takes the odd pane.
 */
export function dockColumns(tiled: readonly AppKind[], width: number): AppKind[][] {
  if (tiled.length === 0) return [];
  if (tiled.length < 2 || width < DOCK_MAX_COLUMNS * PANE_MIN.width + DOCK_GAP) return [[...tiled]];
  const left = Math.ceil(tiled.length / 2);
  return [tiled.slice(0, left), tiled.slice(left)];
}

/** The left column's width in a two-column dock: the stored share, with both columns at 280 or more. */
export function leftColumnPx(split: number, width: number): number {
  const inner = Math.max(0, width - DOCK_GAP);
  const share = Number.isFinite(split) ? clamp(split, DOCK_SPLIT_MIN, DOCK_SPLIT_MAX) : DOCK_SPLIT_DEFAULT;
  return Math.round(clamp(share * inner, PANE_MIN.width, Math.max(PANE_MIN.width, inner - PANE_MIN.width)));
}

/** What a drag of the column separator stores: the share that puts the left column at `px`. */
export function splitForLeft(s: PanelState, px: number, width: number): PanelState {
  const inner = Math.max(1, width - DOCK_GAP);
  const left = clamp(px, PANE_MIN.width, Math.max(PANE_MIN.width, inner - PANE_MIN.width));
  return { ...s, split: clamp(left / inner, DOCK_SPLIT_MIN, DOCK_SPLIT_MAX) };
}

export const weightOf = (s: PanelState, kind: AppKind): number => clamp(s.weights[kind] ?? 1, PANE_WEIGHT_MIN, PANE_WEIGHT_MAX);

/** The heights of one column's panes in a column `height` px tall: by weight, none under 100 (the column scrolls where they cannot all fit). */
export function rowHeights(s: PanelState, column: readonly AppKind[], height: number): number[] {
  if (column.length === 0) return [];
  const floor = PANE_MIN.height;
  if (height <= floor * column.length) return column.map(() => floor);
  const weights = column.map((k) => weightOf(s, k));
  // Panes that would fall under the minimum are pinned to it and the rest share what is left, until none falls under.
  const pinned = new Set<number>();
  for (;;) {
    const free = height - floor * pinned.size;
    const total = weights.reduce((sum, w, i) => (pinned.has(i) ? sum : sum + w), 0);
    const next = weights.findIndex((w, i) => !pinned.has(i) && (w / total) * free < floor);
    if (next < 0) return weights.map((w, i) => (pinned.has(i) ? floor : (w / total) * free));
    pinned.add(next);
  }
}

/**
 * What a drag of the separator between two panes of a column stores: the pane above becomes `abovePx` tall out of the `bothPx`
 * the two share, neither under 100, and the two weights keep their sum (so no other pane of the column moves).
 */
export function resizeRows(s: PanelState, above: AppKind, below: AppKind, abovePx: number, bothPx: number): PanelState {
  if (bothPx < 2 * PANE_MIN.height) return s;
  const sum = weightOf(s, above) + weightOf(s, below);
  const a = clamp(abovePx, PANE_MIN.height, bothPx - PANE_MIN.height);
  const wa = clamp((sum * a) / bothPx, PANE_WEIGHT_MIN, PANE_WEIGHT_MAX);
  return { ...s, weights: { ...s.weights, [above]: wa, [below]: clamp(sum - wa, PANE_WEIGHT_MIN, PANE_WEIGHT_MAX) } };
}
