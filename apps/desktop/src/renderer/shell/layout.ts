/**
 * Shell geometry (AFW-A01..A09), pure: the container width and the persisted
 * preferences give the three regions' widths. The agent list is 210..400 px
 * (default 260); the centre never narrows below 424; the apps panel is a
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
}

export const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

export function clampListWidth(w: number): number {
  return Math.round(clamp(Number.isFinite(w) ? w : GEOMETRY.listDefault, GEOMETRY.listMin, GEOMETRY.listMax));
}

export function clampPanelRatio(r: number): number {
  return clamp(Number.isFinite(r) ? r : GEOMETRY.panelRatioDefault, 0.2, 0.8);
}

export function computeLayout(input: LayoutInput): Layout {
  const width = Math.max(0, Math.round(input.width));
  const wanted = clampListWidth(input.listWidth);
  const rail = input.listCollapsed || width - wanted < GEOMETRY.singlePaneBelow;
  const listPx = rail ? GEOMETRY.rail : wanted;
  const avail = Math.max(0, width - listPx);
  const panelMax = avail - GEOMETRY.centerMin;
  if (!input.panelOpen) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: false, panelMax, singlePane: rail };
  if (panelMax < GEOMETRY.panelMin) return { rail, listPx, centerPx: avail, panelPx: 0, stacked: true, panelMax, singlePane: true };
  const panelPx = Math.round(clamp(clampPanelRatio(input.panelRatio) * width, GEOMETRY.panelMin, panelMax));
  return { rail, listPx, centerPx: avail - panelPx, panelPx, stacked: false, panelMax, singlePane: rail };
}

/** The ratio that produces a panel of `px` pixels in a window of `width` (what a drag stores). */
export function ratioForPanel(px: number, width: number): number {
  return width > 0 ? clampPanelRatio(px / width) : GEOMETRY.panelRatioDefault;
}
