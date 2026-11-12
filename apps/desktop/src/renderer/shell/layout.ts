/**
 * Shell geometry (AFW-A01..A09, DR-PX-2026-11-11-021 MWS-01), pure: the container width and the persisted
 * preferences give the three regions' widths. The task list is 200..420 px
 * (default 288); the centre never narrows below 424; the apps panel is a
 * persisted ratio of the window width clamped to keep the centre at 424 and
 * itself at 384 or more; below a 448 centre the shell is single-pane with the
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
  /** Largest panel width that keeps the centre at its minimum (for the splitter's aria-valuemax). */
  panelMax: number;
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

export function computeLayout(input: LayoutInput): Layout {
  const width = Math.max(0, Math.round(input.width));
  const wanted = clampListWidth(input.listWidth);
  const rail = input.listCollapsed || width - wanted < GEOMETRY.singlePaneBelow;
  const listPx = rail ? GEOMETRY.rail : wanted;
  const avail = Math.max(0, width - listPx);
  const panelMax = avail - GEOMETRY.centerMin;
  const detailsFits = avail > DETAILS_RAIL.hiddenAtOrBelow;
  if (!input.panelOpen) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: false, panelMax, singlePane: rail, detailsPx: input.detailsOpen === true && detailsFits ? DETAILS_RAIL.width : 0, detailsFits };
  if (panelMax < GEOMETRY.panelMin) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: true, panelMax, singlePane: true, detailsPx: 0, detailsFits };
  const panelPx = Math.round(clamp(clampPanelRatio(input.panelRatio) * width, GEOMETRY.panelMin, panelMax));
  return { rail, listPx, centerPx: avail - panelPx, panelPx, stacked: false, panelMax, singlePane: rail, detailsPx: 0, detailsFits };
}

/** The ratio that produces a panel of `px` pixels in a window of `width` (what a drag stores). */
export function ratioForPanel(px: number, width: number): number {
  return width > 0 ? clampPanelRatio(px / width) : GEOMETRY.panelRatioDefault;
}
