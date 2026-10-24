/**
 * Non-colour tokens (AFW-A11 as superseded by DR-PX-2026-10-24-016): a 4 pt spacing grid with half steps, radii, control heights,
 * type, motion, elevation and z-layers. The type scale, the radius hierarchy, the fonts and the shadows are ZCode's (ZDL-02..05,
 * Apache-2.0, third-party/zcode-ui/NOTICE.md), measured from its real stylesheet in evidence/zcode-ui/reference.
 */

export const SPACE = { 0: 0, 0.5: 2, 1: 4, 1.5: 6, 2: 8, 3: 12, 4: 16, 5: 20, 6: 24, 8: 32, 10: 40, 12: 48 } as const;
export const RADIUS = { 1: 2, 2: 4, 3: 6, 4: 8, 5: 10, 6: 12, 7: 14, full: 9999 } as const;
/** Control heights in px: compact, small, standard, large. */
export const CONTROL = { xs: 20, sm: 24, md: 28, lg: 32 } as const;

export const FONT = {
  /** ZCode's sans stack (the Tailwind default it inherits): the system UI font first, no font file shipped or fetched. */
  sans: 'ui-sans-serif, system-ui, sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"',
  /** ZCode's mono stack, with its CJK fallbacks. */
  mono: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", "Microsoft YaHei UI", "Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", monospace',
} as const;

/** ZCode's `text-ui-*` scale (ZDL-02): one `--ui-font-size` (14 px by default) and fixed offsets; never the root font size. */
export const UI_FONT_SIZE = 14;
export const UI_TEXT = { xl: 4, lg: 2, base: 0, caption: -1, sm: -2, xs: -4 } as const;
export type UiText = keyof typeof UI_TEXT;

/**
 * Type roles: the `text-ui-*` step each role uses and its size and line in px at the default `--ui-font-size`. Lines are 1.5 x the
 * size, the leading ZCode's text inherits (measured: 14 px -> 21 px, 12 px -> 18 px, 10 px -> 15 px). The conversation size is
 * scaled by a Text size setting once a screen exposes it (`conversationType`).
 */
export const TYPE = {
  caption: { ui: "sm", size: 12, line: 18 },
  chrome: { ui: "base", size: 14, line: 21 },
  body: { ui: "base", size: 14, line: 21 },
  title: { ui: "lg", size: 16, line: 24 },
  heading: { ui: "xl", size: 18, line: 27 },
  mono: { ui: "sm", size: 12, line: 18 },
} as const satisfies Record<string, { ui: UiText; size: number; line: number }>;

/** ZCode's radius hierarchy (ZDL-03): the first rounded container is xl, nested ones step down to sm. */
export const ZRADIUS = { sm: 4, md: 6, lg: 8, xl: 12, "2xl": 16 } as const;

/** ZCode's overlay shadows (Tailwind's `shadow-sm`, `shadow-md`, `shadow-lg`, measured on its menu and dialog): the same in both themes. */
export const ZSHADOW = {
  sm: "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)",
  md: "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)",
  lg: "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)",
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

/** Elevation shadows per theme: ZCode's `shadow-sm`, `shadow-md` and `shadow-lg` for light and dark; high contrast keeps its own edges. */
export const ELEVATION = {
  light: { 0: "none", 1: ZSHADOW.sm, 2: ZSHADOW.md, 3: ZSHADOW.lg },
  dark: { 0: "none", 1: ZSHADOW.sm, 2: ZSHADOW.md, 3: ZSHADOW.lg },
  "high-contrast": { 0: "none", 1: "none", 2: "0 0 0 1px #ffffff", 3: "0 0 0 2px #ffffff" },
} as const;

/** The shell geometry of AFW-A01..A08 (px, the pt of the spec). PX-143 moves the top bar to 48 and the list default to 264 (DR-PX-2026-10-24-016 section 5.3). */
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
