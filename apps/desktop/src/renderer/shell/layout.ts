/**
 * Shell geometry (AFW-A01..A09, DR-PX-2026-11-11-021 MWS-01), pure: the container width and the persisted
 * preferences give the three regions' widths. The task list is 200..420 px
 * (default 288); the centre never narrows below 424; the apps panel is a
 * persisted ratio of the window width clamped to keep the conversation at
 * 560 beside it (DOCK_LAYOUT) and itself at 384 or more where that leaves room; below a 448 centre the shell is single-pane with the
 * agent list on a 40 px rail; when the panel is open but cannot sit beside a
 * 424 centre it stacks behind a segmented control.
 */
import { GEOMETRY } from "@modbit/design-tokens";

export interface LayoutInput {
  width: number;
  listWidth: number;
  listCollapsed: boolean;
  panelRatio: number;
  panelOpen: boolean;
  /** The person wants the details rail and a task's conversation is in the centre (MWS-09). */
  detailsOpen?: boolean | undefined;
  /** Two tasks side by side are wanted (MWS-11), with the left column's stored share. */
  split?: { ratio: number } | null | undefined;
}

export interface Layout {
  /** The agent list is a 40 px rail (collapsed by the person, or too little room). */
  rail: boolean;
  listPx: number;
  centerPx: number;
  /** The panel's width beside the conversation; 0 when it is closed or stacked. */
  panelPx: number;
  /** The panel is open but does not fit beside the conversation: Conversation | Apps segments. */
  stacked: boolean;
  /** Largest panel width that keeps the conversation at its minimum beside it (for the splitter's aria-valuemax). */
  panelMax: number;
  /** Smallest panel width the splitter goes to: the panel's preferred minimum, or less where the conversation's minimum leaves less (never under one pane). */
  panelMin: number;
  singlePane: boolean;
  /** The details rail's width beside the conversation, inside the centre; 0 when it is not shown. */
  detailsPx: number;
  /** The task area is wide enough for the rail once nothing else is beside the conversation. */
  detailsFits: boolean;
  /** Split view's two columns inside the centre (MWS-11); null where it is not wanted or two do not fit (the focused task is then shown alone). */
  split: { left: number; right: number } | null;
  /** The task area holds two columns (SPLIT_NEED) with nothing beside them. */
  splitFits: boolean;
}

export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

export function clampListWidth(w: number): number {
  return Math.round(clamp(Number.isFinite(w) ? w : GEOMETRY.listDefault, GEOMETRY.listMin, GEOMETRY.listMax));
}

/** The widest the panel goes: its widen button goes here and back. */
export const PANEL_MAX_RATIO = 0.8;

export function clampPanelRatio(r: number): number {
  return clamp(Number.isFinite(r) ? r : GEOMETRY.panelRatioDefault, 0.2, PANEL_MAX_RATIO);
}

/**
 * The details rail (DR-PX-2026-11-11-021 section 5.1, MWS-09): 320 px, never under 280, hidden when the task area is 800 px
 * or narrower. It gives way while the apps panel is open.
 */
export const DETAILS_RAIL = { width: 320, min: 280, hiddenAtOrBelow: 800 } as const;

/**
 * The pane dock beside the conversation (REQ-PX-202, the owner's review of 2026-11-13: "Fix title bar first"). While panes are
 * open the conversation keeps 560 px, two pane minimums (section 5.1: a pane is never under 280), so its title bar holds the
 * title at 200 px or more beside Details, Changes, Terminal, Panes and More. The dock gives first: it drops to one column (it
 * needs 561 for two), then narrows down to one pane's 280; where even that leaves the conversation under 560 the dock stacks
 * behind Conversation | Apps. The centre's minimum of 424 (GEOMETRY.centerMin) still holds for the centre with no dock open.
 */
export const DOCK_LAYOUT = { conversationMin: 560, dockMin: 280 } as const;

/**
 * The widths split view keeps (MWS-11, REQ-PX-204). A column is a task's centre and keeps the centre's minimum
 * (GEOMETRY.centerMin, 424, a number the record keeps); a column beside an open dock keeps the conversation's 560 of
 * REQ-PX-202, so the dock never squeezes a task under what it needs. Two columns therefore need 848 of task area with no dock
 * beside them, and 2 x 560 plus one pane's 280 with the dock beside; under that the dock stacks behind Conversation | Apps as it
 * does for one task. The details rail does not stand beside two columns: each task's facts are under its own More.
 */
export const SPLIT_LAYOUT = { columnMin: GEOMETRY.centerMin, columnMinBesideDock: DOCK_LAYOUT.conversationMin, ratioDefault: 0.5, ratioMin: 0.2, ratioMax: 0.8 } as const;

/** The task area two columns need: two column minimums (848). */
export const SPLIT_NEED = SPLIT_LAYOUT.columnMin * 2;

export function clampSplitRatio(r: unknown): number {
  return typeof r === "number" && Number.isFinite(r) ? clamp(r, SPLIT_LAYOUT.ratioMin, SPLIT_LAYOUT.ratioMax) : SPLIT_LAYOUT.ratioDefault;
}

/** The two columns' widths in a centre of `centerPx`: the stored ratio, clamped so each keeps `min`. Null where two do not fit. */
export function splitColumns(centerPx: number, ratio: number, min: number): { left: number; right: number } | null {
  const total = Math.max(0, Math.round(centerPx));
  if (total < min * 2) return null;
  const left = Math.round(clamp(clampSplitRatio(ratio) * total, min, total - min));
  return { left, right: total - left };
}

export function computeLayout(input: LayoutInput): Layout {
  const width = Math.max(0, Math.round(input.width));
  const wanted = clampListWidth(input.listWidth);
  const rail = input.listCollapsed || width - wanted < GEOMETRY.singlePaneBelow;
  const listPx = rail ? GEOMETRY.rail : wanted;
  const avail = Math.max(0, width - listPx);
  const detailsFits = avail > DETAILS_RAIL.hiddenAtOrBelow;
  const splitFits = avail >= SPLIT_NEED;
  if (input.split && splitFits) return splitLayout(input, width, rail, listPx, avail, input.split.ratio);
  if (!input.panelOpen) {
    const panelMax = avail - GEOMETRY.centerMin;
    return { rail, listPx, centerPx: avail, panelPx: 0, stacked: false, panelMax, panelMin: Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax)), singlePane: rail, detailsPx: input.detailsOpen === true && detailsFits ? DETAILS_RAIL.width : 0, detailsFits, split: null, splitFits };
  }
  const panelMax = avail - DOCK_LAYOUT.conversationMin;
  const panelMin = Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax));
  if (panelMax < DOCK_LAYOUT.dockMin) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: true, panelMax, panelMin, singlePane: true, detailsPx: 0, detailsFits, split: null, splitFits };
  const panelPx = Math.round(clamp(clampPanelRatio(input.panelRatio) * width, panelMin, panelMax));
  return { rail, listPx, centerPx: avail - panelPx, panelPx, stacked: false, panelMax, panelMin, singlePane: rail, detailsPx: 0, detailsFits, split: null, splitFits };
}

/** Two columns in the centre (the task area holds SPLIT_NEED): the dock beside them where each keeps 560, else stacked. */
function splitLayout(input: LayoutInput, width: number, rail: boolean, listPx: number, avail: number, ratio: number): Layout {
  const base = { rail, listPx, detailsPx: 0, detailsFits: avail > DETAILS_RAIL.hiddenAtOrBelow, splitFits: true };
  if (!input.panelOpen) {
    const panelMax = avail - SPLIT_LAYOUT.columnMinBesideDock * 2;
    return { ...base, centerPx: avail, panelPx: 0, stacked: false, panelMax, panelMin: Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax)), singlePane: rail, split: splitColumns(avail, ratio, SPLIT_LAYOUT.columnMin) };
  }
  const panelMax = avail - SPLIT_LAYOUT.columnMinBesideDock * 2;
  const panelMin = Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax));
  if (panelMax < DOCK_LAYOUT.dockMin) return { ...base, centerPx: avail, panelPx: 0, stacked: true, panelMax, panelMin, singlePane: true, split: splitColumns(avail, ratio, SPLIT_LAYOUT.columnMin) };
  const panelPx = Math.round(clamp(clampPanelRatio(input.panelRatio) * width, panelMin, panelMax));
  const centerPx = avail - panelPx;
  return { ...base, centerPx, panelPx, stacked: false, panelMax, panelMin, singlePane: rail, split: splitColumns(centerPx, ratio, SPLIT_LAYOUT.columnMinBesideDock) };
}

/**
 * Whether the Conversation | Apps segment is drawn: exactly when the layout is stacked. The shell's `data-stacked`, the segment
 * and which of the two regions fills the centre all read this one answer of `computeLayout`, never a width of their own, so a
 * stacked shell always has its way to the panes (REQ-PX-202).
 */
export const showsStackSegments = (layout: Pick<Layout, "stacked">): boolean => layout.stacked;

/** The ratio that produces a panel of `px` pixels in a window of `width` (what a drag stores). */
export function ratioForPanel(px: number, width: number): number {
  return width > 0 ? clampPanelRatio(px / width) : GEOMETRY.panelRatioDefault;
}
