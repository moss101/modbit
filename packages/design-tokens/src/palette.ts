/**
 * Modbit's colour roles (AFW-A11, as superseded by DR-PX-2026-10-24-016). Every theme has exactly the same key set: the
 * `ColorRoles` record type makes a missing or extra role a compile error, and `index.test.ts` asserts it at run time.
 *
 * Light and dark are derived from ZCode's Zai Light and Zai Dark tokens (`design-language.ts`, Apache-2.0, see
 * third-party/zcode-ui/NOTICE.md) through ONE explicit role map (`rolesFrom`): each role names the ZCode token it takes and, for
 * a translucent token, the surface it is composited over, so a role is exactly the pixels ZCode shows. The five places where
 * ZCode's own value would fall under AFW-A12 are explicit values that name their exception (ZDX-03 control edge, ZDX-04 focus,
 * ZDX-05 solid-button labels); the computed contrast test (`pairs.ts`) proves every pair in every theme. The high-contrast
 * palette is Modbit's own (ZDX-06): ZCode has none.
 */
import { over } from "./contrast.ts";
import { ZDL_DARK, ZDL_LIGHT, type ZdlColors } from "./design-language.ts";

export type ThemeName = "light" | "dark" | "high-contrast";

export const COLOR_ROLES = [
  // Surfaces, from the window root up.
  "bg",
  "panel",
  "raised",
  "sunken",
  "inverse",
  // Text.
  "text",
  "textMuted",
  /** Disabled and decorative use only (AFW-A12): never a text pair the primitives rely on. */
  "textDisabled",
  "textOnAccent",
  "textOnInverse",
  /** Icons and chevrons that ZCode draws at its subtle foreground level: 3:1, never text (ZDX-01). */
  "iconMuted",
  // Edges and focus.
  /** A decorative edge: cards, panels, menus, dialogs, separators. ZCode's own `border`. */
  "border",
  /** The edge of an interactive control (input, select, outline button, checkbox, radio, switch track): 3:1 (ZDX-03). */
  "borderControl",
  "focus",
  // Accent.
  "accent",
  "accentHover",
  "accentText",
  // Status text/icon colours and their tinted backgrounds.
  "ok",
  "warn",
  "danger",
  "info",
  "okTint",
  "warnTint",
  "dangerTint",
  "infoTint",
  // The solid destructive button and its label (ZDX-05). ZCode's solid warning variant is not adopted: Modbit renders none.
  "dangerSolid",
  "textOnDangerSolid",
  // Composer-mode tints (AFW-D04): never the only signal, always with an icon and a label.
  "modeAsk",
  "modePlan",
  "modeDebug",
  "modeMultitask",
  // Selection and hover fills (opaque; the translucent ZCode token composited over the panel).
  "hover",
  "selected",
  // Scrim behind modal layers (not a text background).
  "scrim",
] as const;
export type ColorRole = (typeof COLOR_ROLES)[number];
export type ColorRoles = Record<ColorRole, string>;

/** Values that are not a ZCode token as ZCode wrote it, each tied to its declared exception in DR-PX-2026-10-24-016 section 5.2. */
interface Exceptions {
  /** ZDX-03: at least 3:1 on every surface, because ZCode's `border` is 1.25:1 (light) and 1.36:1 (dark). */
  borderControl: string;
  /** ZDX-04: ZCode removes every focus ring; Modbit keeps a visible 2 px ring in this colour. */
  focus: string;
  /** ZDX-05: ZCode's white label on its solid destructive button is 3.03:1 in dark. */
  textOnDangerSolid: string;
  scrim: string;
}

/** The one role map: light and dark share it, so their key sets and meanings cannot drift apart. */
function rolesFrom(z: ZdlColors, x: Exceptions): ColorRoles {
  const panel = z.background;
  return {
    bg: z["background-win-alt"],
    panel,
    raised: z.card,
    sunken: over(z.surface, panel),
    inverse: z.tooltip,
    text: z.foreground,
    textMuted: z["remote-foreground-muted"],
    textDisabled: over(z["foreground-subtlest"], panel),
    textOnAccent: z["primary-foreground"],
    textOnInverse: z["tooltip-foreground"],
    iconMuted: over(z["foreground-subtle"], panel),
    border: over(z.border, panel),
    borderControl: x.borderControl,
    focus: x.focus,
    accent: z.primary,
    accentHover: over(withAlpha(z.primary, 0.8), panel),
    accentText: z["interaction-ask-foreground"],
    ok: z["success-surface-foreground"],
    warn: z["warning-surface-foreground"],
    danger: z["destructive-surface-foreground"],
    info: z["progress-surface-foreground"],
    okTint: over(z["success-surface"], panel),
    warnTint: over(z["warning-surface"], panel),
    dangerTint: over(z["destructive-surface"], panel),
    infoTint: over(z["progress-surface"], panel),
    dangerSolid: z.destructive,
    textOnDangerSolid: x.textOnDangerSolid,
    modeAsk: z["success-surface-foreground"],
    modePlan: z["warning-surface-foreground"],
    modeDebug: z["destructive-surface-foreground"],
    modeMultitask: z["remote-docker-foreground"],
    hover: over(z.hover, panel),
    selected: over(z.selected, panel),
    scrim: x.scrim,
  };
}

/** An opaque `#rrggbb` token at a given alpha, as ZCode's `bg-primary/80` hover state draws it. */
function withAlpha(hex: string, alpha: number): string {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

const light: ColorRoles = rolesFrom(ZDL_LIGHT, {
  borderControl: "#858585",
  focus: "#0066dd",
  textOnDangerSolid: ZDL_LIGHT["destructive-foreground"],
  scrim: "#0b0d12",
});

const dark: ColorRoles = rolesFrom(ZDL_DARK, {
  borderControl: "#797979",
  focus: "#80beff",
  textOnDangerSolid: ZDL_DARK["foreground-inverse"],
  scrim: "#000000",
});

const highContrast: ColorRoles = {
  bg: "#000000",
  panel: "#000000",
  raised: "#0a0a0a",
  sunken: "#000000",
  inverse: "#ffffff",
  text: "#ffffff",
  textMuted: "#e6e6e6",
  textDisabled: "#8c8c8c",
  textOnAccent: "#000000",
  textOnInverse: "#000000",
  iconMuted: "#e6e6e6",
  border: "#8c8c8c",
  borderControl: "#ffffff",
  focus: "#ffd400",
  accent: "#7fd1ff",
  accentHover: "#a8e0ff",
  accentText: "#7fd1ff",
  ok: "#6df28f",
  warn: "#ffd400",
  danger: "#ff9a93",
  info: "#7fd1ff",
  okTint: "#001f0b",
  warnTint: "#2a2200",
  dangerTint: "#2e0a08",
  infoTint: "#00192b",
  dangerSolid: "#ff9a93",
  textOnDangerSolid: "#000000",
  modeAsk: "#6df28f",
  modePlan: "#ffd400",
  modeDebug: "#ff9a93",
  modeMultitask: "#d0bfff",
  hover: "#262626",
  selected: "#1f3a5c",
  scrim: "#000000",
};

export const PALETTES: Record<ThemeName, ColorRoles> = { light, dark, "high-contrast": highContrast };
