/**
 * Renderer-local UI preferences (AFW-A03, A06, A09, A13; MWS-03): list width and
 * collapse, panel ratio, each task's pane dock, theme, zoom and density. A per-viewer
 * convenience in the renderer's own storage, like the notification
 * preferences; never task state and never authoritative. Everything read back
 * is validated and clamped, and a missing or hostile value falls back to the
 * default, so a corrupt entry can never break the shell.
 */
import { GEOMETRY, isDensity, isThemePreference, type Density, type ThemePreference } from "@modbit/design-tokens";
import { clampListWidth, clampPanelRatio } from "./layout.ts";

/** The panes a task can show (DR-PX-2026-11-11-021 MWS-10): the five apps PX-048 owns, then Activity, Background tasks and Pull request. */
export const APP_KINDS = ["changes", "terminal", "browser", "files", "evidence", "activity", "background", "pull-request"] as const;
export type AppKind = (typeof APP_KINDS)[number];

/**
 * One task's pane dock (MWS-10): which panes are open, in the order they tile, which of them are minimized, and their sizes.
 * A view choice of this window; the Core knows nothing of it.
 */
export interface PanelState {
  /** The open panes, in the order they tile. */
  panes: AppKind[];
  /** Open panes taken out of the grid; each keeps its place and its size for when it comes back. */
  minimized: AppKind[];
  /** The set the dock's own toggle brings back after every pane was closed. */
  last: AppKind[];
  /** The left column's share of the dock's width where two columns tile. */
  split: number;
  /** A pane's share of its column's height against the others in it; a pane with no entry counts 1. */
  weights: Partial<Record<AppKind, number>>;
}

export const DOCK_SPLIT_DEFAULT = 0.5;
export const DOCK_SPLIT_MIN = 0.2;
export const DOCK_SPLIT_MAX = 0.8;
export const PANE_WEIGHT_MIN = 0.05;
export const PANE_WEIGHT_MAX = 20;
export const EMPTY_DOCK: PanelState = { panes: [], minimized: [], last: [], split: DOCK_SPLIT_DEFAULT, weights: {} };

const isAppKind = (v: unknown): v is AppKind => APP_KINDS.some((k) => k === v);
/** A stored list of panes: known kinds only, each once, in the stored order. */
const paneList = (v: unknown): AppKind[] => (Array.isArray(v) ? [...new Set(v.filter(isAppKind))] : []);

/**
 * Validates one stored dock. An entry from before the dock (`{ open, tab }`: one app behind a tab strip) becomes that one pane,
 * open or remembered (DR-PX-2026-11-11-021 section 7). Null where the entry is neither shape.
 */
export function parsePanelState(s: unknown): PanelState | null {
  if (typeof s !== "object" || s === null || Array.isArray(s)) return null;
  const v = s as Record<string, unknown>;
  if (Array.isArray(v.panes)) {
    const panes = paneList(v.panes);
    const weights: Partial<Record<AppKind, number>> = {};
    if (typeof v.weights === "object" && v.weights !== null && !Array.isArray(v.weights)) {
      for (const [k, w] of Object.entries(v.weights as Record<string, unknown>)) if (isAppKind(k) && typeof w === "number" && Number.isFinite(w)) weights[k] = Math.min(PANE_WEIGHT_MAX, Math.max(PANE_WEIGHT_MIN, w));
    }
    return {
      panes,
      minimized: paneList(v.minimized).filter((k) => panes.includes(k)),
      last: paneList(v.last),
      split: typeof v.split === "number" && Number.isFinite(v.split) ? Math.min(DOCK_SPLIT_MAX, Math.max(DOCK_SPLIT_MIN, v.split)) : DOCK_SPLIT_DEFAULT,
      weights,
    };
  }
  if (typeof v.open !== "boolean") return null;
  const tab = APP_KINDS.find((k) => k === v.tab) ?? "changes";
  return { ...EMPTY_DOCK, panes: v.open ? [tab] : [], last: [tab] };
}

export interface UiPrefs {
  listWidth: number;
  listCollapsed: boolean;
  panelRatio: number;
  panelByTask: Record<string, PanelState>;
  theme: ThemePreference;
  /** UI zoom as a multiple of 1, in steps of 0.08 (AFW-A16, PX-094). */
  zoom: number;
  /** Comfortable or Compact (DR-PX-2026-11-11-021 MWS-03): one setting, separate from zoom. A profile stored before it existed reads as Comfortable. */
  density: Density;
  /** The details rail beside the conversation is shown where it fits (MWS-09). A profile stored before it existed reads as shown. */
  detailsOpen: boolean;
}

export const UI_PREFS_KEY = "modbit.ui.v1";
export const ZOOM_STEP = 0.08;
export const ZOOM_MIN = 0.68;
export const ZOOM_MAX = 1.64;
const MAX_TASK_ENTRIES = 200;

export const DEFAULT_UI_PREFS: UiPrefs = { listWidth: GEOMETRY.listDefault, listCollapsed: false, panelRatio: GEOMETRY.panelRatioDefault, panelByTask: {}, theme: "system", zoom: 1, density: "comfortable", detailsOpen: true };

export function clampZoom(z: number): number {
  const stepped = 1 + Math.round((z - 1) / ZOOM_STEP) * ZOOM_STEP;
  return Math.round(Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, stepped)) * 100) / 100;
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** Validates untrusted stored JSON into preferences. */
export function parseUiPrefs(raw: string | null): UiPrefs {
  if (!raw) return DEFAULT_UI_PREFS;
  let v: unknown;
  try {
    v = JSON.parse(raw);
  } catch {
    return DEFAULT_UI_PREFS;
  }
  if (!isObject(v)) return DEFAULT_UI_PREFS;
  const panelByTask: Record<string, PanelState> = {};
  if (isObject(v.panelByTask)) {
    for (const [id, s] of Object.entries(v.panelByTask).slice(-MAX_TASK_ENTRIES)) {
      const dock = parsePanelState(s);
      if (dock) panelByTask[id] = dock;
    }
  }
  return {
    listWidth: typeof v.listWidth === "number" ? clampListWidth(v.listWidth) : DEFAULT_UI_PREFS.listWidth,
    listCollapsed: v.listCollapsed === true,
    panelRatio: typeof v.panelRatio === "number" ? clampPanelRatio(v.panelRatio) : DEFAULT_UI_PREFS.panelRatio,
    panelByTask,
    theme: isThemePreference(v.theme) ? v.theme : "system",
    zoom: typeof v.zoom === "number" && Number.isFinite(v.zoom) ? clampZoom(v.zoom) : 1,
    density: isDensity(v.density) ? v.density : DEFAULT_UI_PREFS.density,
    detailsOpen: v.detailsOpen !== false,
  };
}

export function serializeUiPrefs(p: UiPrefs): string {
  const entries = Object.entries(p.panelByTask).slice(-MAX_TASK_ENTRIES);
  return JSON.stringify({ ...p, panelByTask: Object.fromEntries(entries) });
}

export function loadUiPrefs(): UiPrefs {
  try {
    return parseUiPrefs(localStorage.getItem(UI_PREFS_KEY));
  } catch {
    return DEFAULT_UI_PREFS;
  }
}

export function saveUiPrefs(p: UiPrefs): void {
  try {
    localStorage.setItem(UI_PREFS_KEY, serializeUiPrefs(p));
  } catch {
    // a per-viewer convenience: nothing depends on it persisting
  }
}
