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

export function computeLayout(input: LayoutInput): Layout {
  const width = Math.max(0, Math.round(input.width));
  const wanted = clampListWidth(input.listWidth);
  const rail = input.listCollapsed || width - wanted < GEOMETRY.singlePaneBelow;
  const listPx = rail ? GEOMETRY.rail : wanted;
  const avail = Math.max(0, width - listPx);
  const detailsFits = avail > DETAILS_RAIL.hiddenAtOrBelow;
  if (!input.panelOpen) {
    const panelMax = avail - GEOMETRY.centerMin;
    return { rail, listPx, centerPx: avail, panelPx: 0, stacked: false, panelMax, panelMin: Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax)), singlePane: rail, detailsPx: input.detailsOpen === true && detailsFits ? DETAILS_RAIL.width : 0, detailsFits };
  }
  const panelMax = avail - DOCK_LAYOUT.conversationMin;
  const panelMin = Math.min(GEOMETRY.panelMin, Math.max(DOCK_LAYOUT.dockMin, panelMax));
  if (panelMax < DOCK_LAYOUT.dockMin) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: true, panelMax, panelMin, singlePane: true, detailsPx: 0, detailsFits };
  const panelPx = Math.round(clamp(clampPanelRatio(input.panelRatio) * width, panelMin, panelMax));
  return { rail, listPx, centerPx: avail - panelPx, panelPx, stacked: false, panelMax, panelMin, singlePane: rail, detailsPx: 0, detailsFits };
}

/** The ratio that produces a panel of `px` pixels in a window of `width` (what a drag stores). */
export function ratioForPanel(px: number, width: number): number {
  return width > 0 ? clampPanelRatio(px / width) : GEOMETRY.panelRatioDefault;
}
