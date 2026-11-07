/**
 * The agent workspace language (DR-PX-2026-11-03-019, AWL-01..03): Modbit's own colour inputs for light and dark, and the one
 * derivation that turns them into every `--color-*` variable the renderer reads. The variable names are the internal names
 * DR-PX-2026-10-24-016 introduced (generic words); their values are this record's, chosen from the AFW-A11 constraints and
 * proven by the computed contrast test (`pairs.ts`). No value here is taken from the reference product or from ZCode.
 */
import { over } from "./contrast.ts";
import { ZDL_TOKENS, type ZdlColors, type ZdlToken } from "./design-language.ts";

/** The inputs of one theme: three achromatic surfaces, one base, the accent and the status and mode hues (AWL-01, AWL-03). */
export interface AwlInputs {
  /** The window root, the top bars and the conversation. */
  chrome: string;
  /** The agent list. */
  sidebar: string;
  /** The apps panel, cards, the composer, menus, popovers and dialogs. */
  editor: string;
  /** Every neutral derives from it (AWL-02). */
  base: string;
  accent: string;
  onAccent: string;
  /** Status text and icon hues: 4.5:1 on every surface. */
  ok: string;
  warn: string;
  danger: string;
  info: string;
  /** The multitask mode and skill hue. */
  purple: string;
  /** The solid destructive fill and its label (ZDX-05 carried over). */
  dangerSolid: string;
  onDangerSolid: string;
  /** Muted text: the 60 % step where it reaches 4.5:1 on every surface, else the darkest step that does (AWL-02). */
  muted: string;
  /** The 3:1 interactive control edge (ZDX-03 carried over). */
  control: string;
  terminal: Record<"black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white", [string, string]>;
}

export const AWL_LIGHT: AwlInputs = {
  chrome: "#f6f6f6",
  sidebar: "#f0f0f0",
  editor: "#fdfdfd",
  base: "#171717",
  accent: "#145fb4",
  onAccent: "#ffffff",
  ok: "#17703f",
  warn: "#8a5100",
  danger: "#b51f3f",
  info: "#145fb4",
  purple: "#6a4fc0",
  dangerSolid: "#c42347",
  onDangerSolid: "#ffffff",
  muted: "#626262",
  control: "#7c7c7c",
  terminal: {
    black: ["#3d3d3d", "#6b6b6b"],
    red: ["#b51f3f", "#c9344f"],
    green: ["#17703f", "#1f8a4d"],
    yellow: ["#8a5100", "#a36400"],
    blue: ["#145fb4", "#2b74c9"],
    magenta: ["#8a2f9e", "#a043b5"],
    cyan: ["#0f6f78", "#14828c"],
    white: ["#8f8f8f", "#171717"],
  },
};

export const AWL_DARK: AwlInputs = {
  chrome: "#131313",
  sidebar: "#181818",
  editor: "#1b1b1b",
  base: "#ececec",
  accent: "#64a8f0",
  onAccent: "#10151c",
  ok: "#5fc488",
  warn: "#e8b162",
  danger: "#f2768f",
  info: "#6fb0f2",
  purple: "#a593f5",
  dangerSolid: "#f2768f",
  onDangerSolid: "#10151c",
  muted: "#a3a3a3",
  control: "#7a7a7a",
  terminal: {
    black: ["#5c5c5c", "#7d7d7d"],
    red: ["#f2768f", "#f69aac"],
    green: ["#5fc488", "#86d6a5"],
    yellow: ["#e8b162", "#f0c788"],
    blue: ["#6fb0f2", "#96c6f6"],
    magenta: ["#d48ae0", "#e2abe9"],
    cyan: ["#5cc3cc", "#86d5db"],
    white: ["#bdbdbd", "#ececec"],
  },
};

/** A `#rrggbb` colour at an alpha, as an `rgba()` literal. */
export function alpha(hex: string, a: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
}

/** The base at one step of the AWL-02 ladder, composited over a surface: what the person sees, as an opaque colour. */
export const step = (i: AwlInputs, a: number, surface: string): string => over(alpha(i.base, a), surface);

/** Every `--color-*` variable of one theme, derived from its inputs. */
export function awlColors(i: AwlInputs): ZdlColors {
  const b = (a: number) => alpha(i.base, a);
  const tint = (hue: string, a: number) => over(alpha(hue, a), i.editor);
  const t = i.terminal;
  const out: Record<ZdlToken, string> = {
    accent: tint(i.accent, 0.12),
    background: i.chrome,
    "background-alt": i.chrome,
    "background-win-alt": i.chrome,
    border: b(0.12),
    "border-hover": b(0.2),
    brand: i.base,
    card: i.editor,
    "card-border": b(0.12),
    "card-selected": i.editor,
    "command-node": b(0.08),
    "command-node-foreground": i.muted,
    "command-node-hover": b(0.14),
    "context-breakdown-1": i.accent,
    "context-breakdown-2": over(alpha(i.accent, 0.85), i.editor),
    "context-breakdown-3": over(alpha(i.accent, 0.7), i.editor),
    "context-breakdown-4": over(alpha(i.accent, 0.55), i.editor),
    "context-breakdown-5": over(alpha(i.accent, 0.4), i.editor),
    "context-breakdown-6": over(alpha(i.accent, 0.28), i.editor),
    "context-breakdown-7": over(alpha(i.accent, 0.16), i.editor),
    destructive: i.dangerSolid,
    "destructive-foreground": i.onDangerSolid,
    "destructive-surface": alpha(i.danger, 0.12),
    "destructive-surface-foreground": i.danger,
    "diff-added": i.ok,
    "diff-added-foreground": i.onAccent,
    "diff-removed": i.danger,
    "diff-removed-foreground": i.onAccent,
    "file-node": alpha(i.info, 0.1),
    "file-node-foreground": i.info,
    "file-node-hover": alpha(i.info, 0.16),
    "find-highlight": tint(i.warn, 0.16),
    "find-highlight-active": tint(i.warn, 0.45),
    foreground: i.base,
    "foreground-inverse": i.chrome,
    "foreground-subtle": b(0.52),
    "foreground-subtlest": b(0.36),
    "git-added": i.ok,
    "git-deleted": i.danger,
    "git-descendant": i.warn,
    "git-ignored": b(0.36),
    "git-modified": i.warn,
    "git-none": i.base,
    "git-renamed": i.info,
    "git-untracked": i.ok,
    header: i.chrome,
    hover: b(0.06),
    "idle-task": i.purple,
    "idle-task-surface": tint(i.purple, 0.12),
    input: i.editor,
    "input-border": b(0.12),
    "input-border-focused": i.accent,
    "input-border-hover": b(0.2),
    "input-focused": i.editor,
    "interaction-ask-fill": alpha(i.ok, 0.2),
    "interaction-ask-foreground": i.accent,
    "interaction-ask-surface": tint(i.accent, 0.12),
    "interaction-confirmation-foreground": i.ok,
    "interaction-confirmation-surface": tint(i.ok, 0.12),
    menu: i.editor,
    "menu-hover": step(i, 0.06, i.editor),
    panel: i.editor,
    popover: i.editor,
    "popover-border": b(0.12),
    "popover-foreground": i.base,
    "popover-header": i.chrome,
    primary: i.accent,
    "primary-foreground": i.onAccent,
    "progress-surface": tint(i.info, 0.12),
    "progress-surface-foreground": i.info,
    "remote-docker-foreground": i.purple,
    "remote-foreground-muted": i.muted,
    "resize-handle": b(0.2),
    secondary: step(i, 0.08, i.chrome),
    selected: b(0.08),
    sidebar: i.sidebar,
    "skill-node": alpha(i.purple, 0.1),
    "skill-node-foreground": i.purple,
    "skill-node-hover": alpha(i.purple, 0.16),
    success: i.ok,
    "success-foreground": i.onAccent,
    "success-surface": tint(i.ok, 0.12),
    "success-surface-foreground": i.ok,
    surface: b(0.04),
    "surface-hover": b(0.06),
    tab: i.chrome,
    "tab-active": i.editor,
    "tab-border": b(0.08),
    tag: step(i, 0.08, i.editor),
    "terminal-bg": i.editor,
    "terminal-black": t.black[0],
    "terminal-blue": t.blue[0],
    "terminal-bright-black": t.black[1],
    "terminal-bright-blue": t.blue[1],
    "terminal-bright-cyan": t.cyan[1],
    "terminal-bright-green": t.green[1],
    "terminal-bright-magenta": t.magenta[1],
    "terminal-bright-red": t.red[1],
    "terminal-bright-white": t.white[1],
    "terminal-bright-yellow": t.yellow[1],
    "terminal-cursor": i.base,
    "terminal-cursor-accent": i.editor,
    "terminal-cyan": t.cyan[0],
    "terminal-fg": i.base,
    "terminal-green": t.green[0],
    "terminal-magenta": t.magenta[0],
    "terminal-red": t.red[0],
    "terminal-selection": alpha(i.accent, 0.22),
    "terminal-selection-inactive": b(0.1),
    "terminal-white": t.white[0],
    "terminal-yellow": t.yellow[0],
    toast: i.editor,
    tooltip: i.editor,
    "tooltip-foreground": i.base,
    "tooltip-tag": step(i, 0.08, i.editor),
    "tooltip-tag-foreground": i.muted,
    warning: i.warn,
    "warning-foreground": i.onAccent,
    "warning-surface": alpha(i.warn, 0.12),
    "warning-surface-foreground": i.warn,
  };
  for (const k of ZDL_TOKENS) if (!(k in out)) throw new Error(`AWL leaves ${k} undefined`);
  return out;
}

export const AWL_COLORS_LIGHT: ZdlColors = awlColors(AWL_LIGHT);
export const AWL_COLORS_DARK: ZdlColors = awlColors(AWL_DARK);
