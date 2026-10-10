/** Non-colour tokens (AFW-A11): a 4 pt spacing grid with half steps, radii, control heights, type, motion, elevation and z-layers. */

export const SPACE = { 0: 0, 0.5: 2, 1: 4, 1.5: 6, 2: 8, 3: 12, 4: 16, 5: 20, 6: 24, 8: 32, 10: 40, 12: 48 } as const;
export const RADIUS = { 1: 2, 2: 4, 3: 6, 4: 8, 5: 10, 6: 12, 7: 14, full: 9999 } as const;
/** Control heights in px: compact, small, standard, large. */
export const CONTROL = { xs: 20, sm: 24, md: 28, lg: 32 } as const;

export const FONT = {
  /** The system font stack only: no font file is shipped or fetched. */
  sans: 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
  mono: 'ui-monospace, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
} as const;

/** Type scale in px: size / line-height. Chrome is 13/18; the conversation size is scaled by the Text size setting. */
export const TYPE = {
  caption: { size: 11, line: 16 },
  chrome: { size: 13, line: 18 },
  body: { size: 14, line: 20 },
  title: { size: 16, line: 22 },
  heading: { size: 20, line: 28 },
  mono: { size: 12, line: 18 },
} as const;

/** The conversation text size for a Text size setting (percent, clamped to 80..140); line pitch stays 24/14 of the size. */
export function conversationType(percent: number): { size: number; line: number } {
  const p = Math.min(140, Math.max(80, Math.round(percent)));
  const size = Math.round((14 * p) / 100);
  return { size, line: Math.round(size * (24 / 14)) };
}

/** Durations in ms. With prefers-reduced-motion every one collapses to 0 (AFW-A14). */
export const MOTION = { instant: 50, fast: 100, base: 150, slow: 200, slower: 300 } as const;
export const PRESS_SCALE = 0.98;

export const Z = { base: 0, panel: 10, topbar: 20, tray: 30, modal: 100, popover: 110, tooltip: 120 } as const;

/** Elevation shadows per theme (dark and high-contrast lean on edges more than blur). */
export const ELEVATION = {
  light: { 0: "none", 1: "0 1px 2px rgb(20 23 28 / 0.12)", 2: "0 4px 12px rgb(20 23 28 / 0.16)", 3: "0 12px 32px rgb(20 23 28 / 0.22)" },
  dark: { 0: "none", 1: "0 1px 2px rgb(0 0 0 / 0.5)", 2: "0 4px 12px rgb(0 0 0 / 0.55)", 3: "0 12px 32px rgb(0 0 0 / 0.65)" },
  "high-contrast": { 0: "none", 1: "none", 2: "0 0 0 1px #ffffff", 3: "0 0 0 2px #ffffff" },
} as const;

/** The shell geometry of AFW-A01..A08 (px, the pt of the spec). */
export const GEOMETRY = {
  topBar: 40,
  listDefault: 260,
  listMin: 210,
  listMax: 400,
  rail: 40,
  centerMin: 424,
  panelMin: 384,
  panelRatioDefault: 0.5,
  singlePaneBelow: 448,
  windowDefault: { width: 1280, height: 800 },
  windowMin: { width: 900, height: 600 },
} as const;
