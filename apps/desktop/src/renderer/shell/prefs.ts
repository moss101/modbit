/**
 * Renderer-local UI preferences (AFW-A03, A06, A09, A13): list width and
 * collapse, panel ratio, per-task panel state, theme and zoom. A per-viewer
 * convenience in the renderer's own storage, like the notification
 * preferences; never task state and never authoritative. Everything read back
 * is validated and clamped, and a missing or hostile value falls back to the
 * default, so a corrupt entry can never break the shell.
 */
import { GEOMETRY, isThemePreference, type ThemePreference } from "@modbit/design-tokens";
import { clampListWidth, clampPanelRatio } from "./layout.ts";

export const APP_KINDS = ["changes", "terminal", "browser", "files", "evidence"] as const;
export type AppKind = (typeof APP_KINDS)[number];

export interface PanelState {
  open: boolean;
  tab: AppKind;
}

export interface UiPrefs {
  listWidth: number;
  listCollapsed: boolean;
  panelRatio: number;
  panelByTask: Record<string, PanelState>;
  theme: ThemePreference;
  /** UI zoom as a multiple of 1, in steps of 0.08 (AFW-A16, PX-094). */
  zoom: number;
}

export const UI_PREFS_KEY = "modbit.ui.v1";
export const ZOOM_STEP = 0.08;
export const ZOOM_MIN = 0.68;
export const ZOOM_MAX = 1.64;
const MAX_TASK_ENTRIES = 200;

export const DEFAULT_UI_PREFS: UiPrefs = { listWidth: GEOMETRY.listDefault, listCollapsed: false, panelRatio: GEOMETRY.panelRatioDefault, panelByTask: {}, theme: "system", zoom: 1 };

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
      if (!isObject(s) || typeof s.open !== "boolean") continue;
      const tab = APP_KINDS.find((k) => k === s.tab) ?? "changes";
      panelByTask[id] = { open: s.open, tab };
    }
  }
  return {
    listWidth: typeof v.listWidth === "number" ? clampListWidth(v.listWidth) : DEFAULT_UI_PREFS.listWidth,
    listCollapsed: v.listCollapsed === true,
    panelRatio: typeof v.panelRatio === "number" ? clampPanelRatio(v.panelRatio) : DEFAULT_UI_PREFS.panelRatio,
    panelByTask,
    theme: isThemePreference(v.theme) ? v.theme : "system",
    zoom: typeof v.zoom === "number" && Number.isFinite(v.zoom) ? clampZoom(v.zoom) : 1,
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
