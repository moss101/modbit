/**
 * Modbit's own colour roles (AFW-A11). Every theme has exactly the same key
 * set: the `ColorRoles` record type makes a missing or extra role a compile
 * error, and `tokens.test.ts` asserts it at run time. Neutral surfaces are
 * derived from one base per theme with `mix`; the semantic colours are chosen
 * here and verified by the computed contrast test (`pairs.ts`), not by eye.
 * No value is taken from any other product.
 */
import { mix } from "./contrast.ts";

export type ThemeName = "light" | "dark" | "high-contrast";

export const COLOR_ROLES = [
  // Surfaces, from the app background up.
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
  // Edges and focus.
  "border",
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
  // Composer-mode tints (AFW-D04): never the only signal, always with an icon and a label.
  "modeAsk",
  "modePlan",
  "modeDebug",
  "modeMultitask",
  // Selection and hover fills (opaque; derived from the surface ladder).
  "hover",
  "selected",
  // Scrim behind modal layers (not a text background).
  "scrim",
] as const;
export type ColorRole = (typeof COLOR_ROLES)[number];
export type ColorRoles = Record<ColorRole, string>;

const LIGHT_BASE = "#f3f4f7";
const DARK_BASE = "#12141a";

const light: ColorRoles = {
  bg: LIGHT_BASE,
  panel: "#ffffff",
  raised: "#ffffff",
  sunken: mix(LIGHT_BASE, "#ffffff", 0.55),
  inverse: "#1c2027",
  text: "#14171c",
  textMuted: "#4a525d",
  textDisabled: "#8a919b",
  textOnAccent: "#ffffff",
  textOnInverse: "#f3f4f7",
  border: "#d4d8df",
  borderControl: "#7a828e",
  focus: "#1d4ed8",
  accent: "#1d4ed8",
  accentHover: "#1a43b8",
  accentText: "#1d4ed8",
  ok: "#1b6e35",
  warn: "#8a4b00",
  danger: "#b3261e",
  info: "#1d4ed8",
  okTint: "#e2f3e7",
  warnTint: "#fcefd9",
  dangerTint: "#fbe6e4",
  infoTint: "#e4ebff",
  modeAsk: "#1b6e35",
  modePlan: "#8a4b00",
  modeDebug: "#b3261e",
  modeMultitask: "#6a3cc2",
  hover: mix(LIGHT_BASE, "#14171c", 0.06),
  selected: "#dfe7fb",
  scrim: "#0b0d12",
};

const dark: ColorRoles = {
  bg: DARK_BASE,
  panel: mix(DARK_BASE, "#ffffff", 0.05),
  raised: mix(DARK_BASE, "#ffffff", 0.09),
  sunken: mix(DARK_BASE, "#000000", 0.35),
  inverse: "#e8eaee",
  text: "#e8eaee",
  textMuted: "#a3acb8",
  textDisabled: "#6a717c",
  textOnAccent: "#0a1020",
  textOnInverse: "#14171c",
  border: "#2c313a",
  borderControl: "#7d8794",
  focus: "#8fb0ff",
  accent: "#8fb0ff",
  accentHover: "#a9c2ff",
  accentText: "#8fb0ff",
  ok: "#5ad27f",
  warn: "#ffb454",
  danger: "#ff7b72",
  info: "#8fb0ff",
  okTint: "#15301e",
  warnTint: "#38290f",
  dangerTint: "#3b1a19",
  infoTint: "#1a2744",
  modeAsk: "#5ad27f",
  modePlan: "#ffb454",
  modeDebug: "#ff7b72",
  modeMultitask: "#b9a2ff",
  hover: mix(DARK_BASE, "#ffffff", 0.1),
  selected: "#243457",
  scrim: "#000000",
};

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
  modeAsk: "#6df28f",
  modePlan: "#ffd400",
  modeDebug: "#ff9a93",
  modeMultitask: "#d0bfff",
  hover: "#262626",
  selected: "#1f3a5c",
  scrim: "#000000",
};

export const PALETTES: Record<ThemeName, ColorRoles> = { light, dark, "high-contrast": highContrast };
