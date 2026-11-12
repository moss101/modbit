/**
 * Non-colour tokens (AFW-A11, DR-PX-2026-11-03-019 AWL-04): a 4 pt spacing grid with half steps, radii, control heights, type,
 * motion, elevation and z-layers. The font stacks and the overlay shadows are the generic system stacks and Tailwind CSS shadows
 * (MIT) that DR-PX-2026-10-24-016 introduced; the radii are that record's; the geometry, the type sizes and the two densities are
 * DR-PX-2026-11-11-021's (the measured workspace, MWS-01 and MWS-03).
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

/**
 * The `text-ui-*` scale (AWL-04): one `--ui-font-size` (chrome type) and fixed offsets; never the root font size. The size is the
 * density's (DR-PX-2026-11-11-021 MWS-03): 14 px in Comfortable, the default, and 13 px in Compact.
 */
export const UI_FONT_SIZE = 14;
export const UI_TEXT = { xl: 4, lg: 1, base: 0, caption: -1, sm: -1, xs: -2 } as const;
export type UiText = keyof typeof UI_TEXT;

type TypeRoles = Record<"caption" | "chrome" | "body" | "title" | "heading" | "mono", { ui: UiText; size: number; line: number }>;

/**
 * Type roles (AWL-04, sizes of MWS-03): the `text-ui-*` step each role uses and its size and line in px in the Comfortable
 * density: chrome 14/20, caption 13/18, the transcript 15/21, title 15/21, heading 18/24, mono 13/20. `COMPACT.type` holds the
 * Compact lines. The conversation size is scaled by a Text size setting once a screen exposes it (`conversationType`).
 */
export const TYPE = {
  caption: { ui: "caption", size: 13, line: 18 },
  chrome: { ui: "base", size: 14, line: 20 },
  body: { ui: "lg", size: 15, line: 21 },
  title: { ui: "lg", size: 15, line: 21 },
  heading: { ui: "xl", size: 18, line: 24 },
  mono: { ui: "sm", size: 13, line: 20 },
} as const satisfies TypeRoles;

/** The radius hierarchy (AWL-04, MWS-01): rows 8, controls 6, cards, menus and dialogs 12, the composer 14. */
export const ZRADIUS = { sm: 4, md: 6, lg: 8, xl: 12, "2xl": 14 } as const;

/** ZCode's overlay shadows (Tailwind's `shadow-sm`, `shadow-md`, `shadow-lg`, measured on its menu and dialog): the same in both themes. */
export const ZSHADOW = {
  sm: "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)",
  md: "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)",
  lg: "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)",
  "2xl": "0 25px 50px -12px rgb(0 0 0 / 0.25)",
} as const;

/** The conversation text size for a Text size setting (percent, clamped to 80..140) in a density; the line keeps the density's pitch. */
export function conversationType(percent: number, density: Density = "comfortable"): { size: number; line: number } {
  const p = Math.min(140, Math.max(80, Math.round(percent)));
  const body = density === "compact" ? COMPACT.type.body : TYPE.body;
  const size = Math.round((body.size * p) / 100);
  return { size, line: Math.round(size * (body.line / body.size)) };
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

/**
 * The shell geometry of AFW-A01..A08 (px, the pt of the spec) in the Comfortable density, as measured for DR-PX-2026-11-11-021
 * section 5.1 (MWS-01): the task list 288 (200 to 420), the top bar and every pane head 45, navigation and task rows 32 on one
 * line and 2 apart with radius 8 and inset 8, the conversation's column 768 with its text at 736, the composer's radius 14. The
 * centre's minimum, the single-pane threshold, the rail and the window's sizes keep the numbers of DR-PX-2026-11-03-019.
 */
export const GEOMETRY = {
  topBar: 45,
  listDefault: 288,
  listMin: 200,
  listMax: 420,
  rail: 40,
  centerMin: 424,
  panelMin: 384,
  panelRatioDefault: 0.39,
  singlePaneBelow: 448,
  /** A navigation row and a one-line task row. */
  row: 32,
  rowGap: 2,
  rowRadius: 8,
  rowInset: 8,
  /** A task row that keeps its second line (MWS-04). */
  rowTwoLine: 46,
  /** A group's heading in the task list, with the space above it. */
  groupHead: 38,
  listFoot: 48,
  column: 768,
  columnText: 736,
  composerRadius: 14,
  windowDefault: { width: 1280, height: 800 },
  windowMin: { width: 900, height: 600 },
} as const;

/** The two densities of MWS-03: one setting, stored with the layout preferences and separate from zoom. */
export const DENSITIES = ["comfortable", "compact"] as const;
export type Density = (typeof DENSITIES)[number];
export const DENSITY_LABEL: Record<Density, string> = { comfortable: "Comfortable", compact: "Compact" };
export const isDensity = (v: unknown): v is Density => typeof v === "string" && (DENSITIES as readonly string[]).includes(v);

/**
 * What Compact overrides (MWS-03): chrome text 13 on 18, rows 30, bars 40, the transcript 14 on 20. Everything else is shared.
 * Compact is these token values and nothing else: no component sets a size of its own.
 */
export const COMPACT = {
  uiFontSize: 13,
  type: {
    caption: { ui: "caption", size: 12, line: 16 },
    chrome: { ui: "base", size: 13, line: 18 },
    body: { ui: "lg", size: 14, line: 20 },
    title: { ui: "lg", size: 14, line: 20 },
    heading: { ui: "xl", size: 17, line: 24 },
    mono: { ui: "sm", size: 12, line: 20 },
  },
  topBar: 40,
  row: 30,
  rowTwoLine: 44,
  groupHead: 32,
} as const satisfies { uiFontSize: number; type: TypeRoles; topBar: number; row: number; rowTwoLine: number; groupHead: number };

/** The sizes a density gives the shell, for the code that must know them as numbers (the virtual task list's row heights). */
export interface DensityMetrics {
  topBar: number;
  row: number;
  rowGap: number;
  rowTwoLine: number;
  groupHead: number;
}

export function densityMetrics(density: Density): DensityMetrics {
  const base = { topBar: GEOMETRY.topBar, row: GEOMETRY.row, rowGap: GEOMETRY.rowGap, rowTwoLine: GEOMETRY.rowTwoLine, groupHead: GEOMETRY.groupHead };
  return density === "compact" ? { ...base, topBar: COMPACT.topBar, row: COMPACT.row, rowTwoLine: COMPACT.rowTwoLine, groupHead: COMPACT.groupHead } : base;
}
