/**
 * Non-colour tokens (AFW-A11, DR-PX-2026-11-03-019 AWL-04): a 4 pt spacing grid with half steps, radii, control heights, type,
 * motion, elevation and z-layers. The font stacks and the overlay shadows are the generic system stacks and Tailwind CSS shadows
 * (MIT) that DR-PX-2026-10-24-016 introduced; the type scale, radii and geometry are this record's.
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

/** The `text-ui-*` scale (AWL-04): one `--ui-font-size` (13 px, chrome type) and fixed offsets; never the root font size. */
export const UI_FONT_SIZE = 13;
export const UI_TEXT = { xl: 4, lg: 1, base: 0, caption: -1, sm: -1, xs: -2 } as const;
export type UiText = keyof typeof UI_TEXT;

/**
 * Type roles (AWL-04): the `text-ui-*` step each role uses and its size and line in px at the default `--ui-font-size`: chrome
 * 13/18, caption 12/16, conversation body 14/22, title 14/20, heading 17/24, mono 12/20. The conversation size is scaled by a
 * Text size setting once a screen exposes it (`conversationType`).
 */
export const TYPE = {
  caption: { ui: "caption", size: 12, line: 16 },
  chrome: { ui: "base", size: 13, line: 18 },
  body: { ui: "lg", size: 14, line: 22 },
  title: { ui: "lg", size: 14, line: 20 },
  heading: { ui: "xl", size: 17, line: 24 },
  mono: { ui: "sm", size: 12, line: 20 },
} as const satisfies Record<string, { ui: UiText; size: number; line: number }>;

/** The radius hierarchy (AWL-04): rows 8, controls 6, cards, the composer, menus and dialogs 12. */
export const ZRADIUS = { sm: 4, md: 6, lg: 8, xl: 12, "2xl": 14 } as const;

/** ZCode's overlay shadows (Tailwind's `shadow-sm`, `shadow-md`, `shadow-lg`, measured on its menu and dialog): the same in both themes. */
export const ZSHADOW = {
  sm: "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)",
  md: "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)",
  lg: "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)",
  "2xl": "0 25px 50px -12px rgb(0 0 0 / 0.25)",
} as const;

/** The conversation text size for a Text size setting (percent, clamped to 80..140); line pitch stays 22/14 of the size. */
export function conversationType(percent: number): { size: number; line: number } {
  const p = Math.min(140, Math.max(80, Math.round(percent)));
  const size = Math.round((14 * p) / 100);
  return { size, line: Math.round(size * (22 / 14)) };
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

/** The shell geometry of AFW-A01..A08 (px, the pt of the spec), as restored by DR-PX-2026-11-03-019 section 5.3. */
export const GEOMETRY = {
  topBar: 40,
  listDefault: 260,
  listMin: 210,
  listMax: 400,
  rail: 40,
  centerMin: 424,
  panelMin: 384,
  panelRatioDefault: 0.39,
  singlePaneBelow: 448,
  windowDefault: { width: 1280, height: 800 },
  windowMin: { width: 900, height: 600 },
} as const;
