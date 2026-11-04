/**
 * Modbit's colour roles (AFW-A11, DR-PX-2026-11-03-019). Every theme has exactly the same key set: the `ColorRoles` record type
 * makes a missing or extra role a compile error, and `index.test.ts` asserts it at run time.
 *
 * Light and dark are derived from the agent workspace language inputs (`workspace-language.ts`, Modbit's own values: three
 * achromatic surfaces, one base and the AFW-A11 ladder, one blue accent) through ONE explicit role map (`rolesFrom`); a
 * translucent step is composited over the surface it sits on, so a role is exactly the pixels shown. The computed contrast test
 * (`pairs.ts`) proves every pair in every theme. The high-contrast palette is Modbit's own and unchanged.
 */
import { mix, over } from "./contrast.ts";
import { AWL_DARK, AWL_LIGHT, alpha, step, type AwlInputs } from "./workspace-language.ts";

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
  /** Icons and chevrons at the 52 % icon step: 3:1, never text (AWL-02). */
  "iconMuted",
  // Edges and focus.
  /** A decorative edge: cards, panels, menus, dialogs, separators (stroke 12 %). */
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
  // The solid destructive button and its label (ZDX-05 carried over). Modbit renders no solid warning button.
  "dangerSolid",
  "textOnDangerSolid",
  // Composer-mode tints (AFW-D04): never the only signal, always with an icon and a label.
  "modeAsk",
  "modePlan",
  "modeDebug",
  "modeMultitask",
  // Selection and hover fills (opaque; the 6 % and 8 % fill steps composited over the panel).
  "hover",
  "selected",
  // Scrim behind modal layers (not a text background).
  "scrim",
] as const;
export type ColorRole = (typeof COLOR_ROLES)[number];
export type ColorRoles = Record<ColorRole, string>;

/**
 * The one role map (AWL-01..03): light and dark share it, so their key sets and meanings cannot drift apart. `panel` is the
 * conversation surface (chrome), `raised` the editor surface (apps panel, cards, composer, menus), `bg` the window root.
 */
function rolesFrom(i: AwlInputs): ColorRoles {
  const panel = i.chrome;
  return {
    bg: i.chrome,
    panel,
    raised: i.editor,
    sunken: step(i, 0.04, panel),
    inverse: i.base,
    text: i.base,
    textMuted: i.muted,
    textDisabled: step(i, 0.36, panel),
    textOnAccent: i.onAccent,
    textOnInverse: i.chrome,
    iconMuted: step(i, 0.52, panel),
    border: step(i, 0.12, panel),
    borderControl: i.control,
    focus: i.accent,
    accent: i.accent,
    accentHover: mix(i.accent, i.base, 0.1),
    accentText: i.accent,
    ok: i.ok,
    warn: i.warn,
    danger: i.danger,
    info: i.info,
    okTint: over(alpha(i.ok, 0.12), panel),
    warnTint: over(alpha(i.warn, 0.12), panel),
    dangerTint: over(alpha(i.danger, 0.12), panel),
    infoTint: over(alpha(i.info, 0.12), panel),
    dangerSolid: i.dangerSolid,
    textOnDangerSolid: i.onDangerSolid,
    modeAsk: i.ok,
    modePlan: i.warn,
    modeDebug: i.danger,
    modeMultitask: i.purple,
    hover: step(i, 0.06, panel),
    selected: step(i, 0.08, panel),
    scrim: "#000000",
  };
}

const light: ColorRoles = rolesFrom(AWL_LIGHT);
const dark: ColorRoles = rolesFrom(AWL_DARK);

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
