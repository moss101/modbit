/**
 * The one typed function that emits Modbit's tokens (AFW-A11): `tokens(theme)`
 * returns the typed set, `cssVariables(theme)` flattens it to custom
 * properties, and `tokensCss()` is the stylesheet the app links: light by
 * default, dark through `prefers-color-scheme`, high contrast through
 * `prefers-contrast: more`, each overridable by `data-theme` on the root, and
 * every duration collapsed under `prefers-reduced-motion` (AFW-A14).
 */
import { PALETTES, type ColorRole, type ColorRoles, type ThemeName } from "./palette.ts";
import { CONTROL, ELEVATION, FONT, GEOMETRY, MOTION, PRESS_SCALE, RADIUS, SPACE, TYPE, UI_FONT_SIZE, UI_TEXT, Z, ZRADIUS, ZSHADOW } from "./scale.ts";
import { ZDL_DARK, ZDL_LIGHT, type ZdlColors, type ZdlToken } from "./design-language.ts";

export interface TokenSet {
  theme: ThemeName;
  color: ColorRoles;
  elevation: Record<0 | 1 | 2 | 3, string>;
  space: typeof SPACE;
  radius: typeof RADIUS;
  control: typeof CONTROL;
  font: typeof FONT;
  type: typeof TYPE;
  motion: typeof MOTION;
  pressScale: number;
  z: typeof Z;
  geometry: typeof GEOMETRY;
}

export function tokens(theme: ThemeName): TokenSet {
  return { theme, color: PALETTES[theme], elevation: ELEVATION[theme], space: SPACE, radius: RADIUS, control: CONTROL, font: FONT, type: TYPE, motion: MOTION, pressScale: PRESS_SCALE, z: Z, geometry: GEOMETRY };
}

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
const step = (k: string | number) => String(k).replace(".", "_");

/**
 * The ZCode token names for the high-contrast theme, which has no ZCode original (ZDX-06): the semantic ones read the
 * high-contrast role they stand for and every other token keeps its Zai Dark value, so no `--color-*` is ever undefined.
 */
const HC_TOKENS: Partial<Record<ZdlToken, ColorRole>> = {
  background: "panel", "background-win-alt": "bg", "background-alt": "bg", panel: "panel", header: "panel", sidebar: "panel", surface: "sunken",
  card: "raised", popover: "raised", menu: "raised", input: "raised", toast: "raised", tab: "panel", "tab-active": "raised", tooltip: "inverse",
  "tooltip-foreground": "textOnInverse", foreground: "text", "foreground-subtle": "textMuted", "foreground-subtlest": "textDisabled",
  "remote-foreground-muted": "textMuted", border: "border", "border-hover": "borderControl", "card-border": "border", "popover-border": "border",
  "input-border": "borderControl", "input-border-hover": "borderControl", "input-border-focused": "focus", hover: "hover", "menu-hover": "hover",
  selected: "selected", "card-selected": "selected", primary: "accent", "primary-foreground": "textOnAccent", brand: "accent",
  destructive: "dangerSolid", "destructive-foreground": "textOnDangerSolid", warning: "warn", "warning-foreground": "textOnInverse",
  success: "ok", "success-foreground": "textOnAccent", "destructive-surface-foreground": "danger", "warning-surface-foreground": "warn",
  "success-surface-foreground": "ok", "progress-surface-foreground": "info", "success-surface": "okTint", "progress-surface": "infoTint",
  "interaction-ask-foreground": "accentText", "interaction-confirmation-foreground": "ok", "resize-handle": "focus",
};

/** ZCode's colour tokens for a theme: Zai Light, Zai Dark, or (high contrast) the dark table with the semantic tokens re-pointed at Modbit's roles. */
export function zdlColors(theme: ThemeName): ZdlColors {
  if (theme === "light") return ZDL_LIGHT;
  if (theme === "dark") return ZDL_DARK;
  const roles = PALETTES["high-contrast"];
  const out: ZdlColors = { ...ZDL_DARK };
  for (const [token, role] of Object.entries(HC_TOKENS) as [ZdlToken, ColorRole][]) out[token] = roles[role];
  return out;
}

/** The theme-dependent variables (colour and elevation): the only ones that differ between themes. */
export function themeVariables(theme: ThemeName): Record<string, string> {
  const t = tokens(theme);
  const out: Record<string, string> = {};
  for (const [role, value] of Object.entries(t.color)) out[`--mb-color-${kebab(role)}`] = value;
  for (const [level, value] of Object.entries(t.elevation)) out[`--mb-elevation-${level}`] = value;
  for (const [name, value] of Object.entries(zdlColors(theme))) out[`--color-${name}`] = value;
  return out;
}

/** The theme-independent variables. */
export function sharedVariables(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(SPACE)) out[`--mb-space-${step(k)}`] = `${v}px`;
  for (const [k, v] of Object.entries(RADIUS)) out[`--mb-radius-${k}`] = `${v}px`;
  for (const [k, v] of Object.entries(CONTROL)) out[`--mb-control-${k}`] = `${v}px`;
  out["--mb-font-sans"] = FONT.sans;
  out["--mb-font-mono"] = FONT.mono;
  out["--font-sans"] = FONT.sans;
  out["--font-mono"] = FONT.mono;
  // ZCode's one-knob type scale: only `--ui-font-size` changes, never the root font size (ZDL-02).
  out["--ui-font-size"] = `${UI_FONT_SIZE}px`;
  for (const [k, offset] of Object.entries(UI_TEXT)) out[`--text-ui-${k}`] = offset === 0 ? "var(--ui-font-size)" : `calc(var(--ui-font-size) ${offset < 0 ? "-" : "+"} ${Math.abs(offset)}px)`;
  for (const [k, v] of Object.entries(TYPE)) {
    out[`--mb-type-${k}-size`] = `var(--text-ui-${v.ui})`;
    out[`--mb-type-${k}-line`] = `calc(var(--text-ui-${v.ui}) * 1.5)`;
  }
  for (const [k, v] of Object.entries(ZRADIUS)) out[`--radius-${k}`] = `${v}px`;
  for (const [k, v] of Object.entries(ZSHADOW)) out[`--shadow-${k}`] = v;
  for (const [k, v] of Object.entries(MOTION)) out[`--mb-motion-${k}`] = `${v}ms`;
  out["--mb-press-scale"] = String(PRESS_SCALE);
  for (const [k, v] of Object.entries(Z)) out[`--mb-z-${k}`] = String(v);
  out["--mb-top-bar"] = `${GEOMETRY.topBar}px`;
  return out;
}

const block = (vars: Record<string, string>, indent = "  "): string =>
  Object.entries(vars)
    .map(([k, v]) => `${indent}${k}: ${v};`)
    .join("\n");

export function tokensCss(): string {
  const shared = block(sharedVariables());
  const zeroMotion = block(Object.fromEntries(Object.keys(MOTION).map((k) => [`--mb-motion-${k}`, "0ms"])), "    ");
  return `/* generated from packages/design-tokens: do not edit */
:root {
  color-scheme: light;
${shared}
${block(themeVariables("light"))}
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme]) {
    color-scheme: dark;
${block(themeVariables("dark"), "    ")}
  }
}
@media (prefers-contrast: more) {
  :root:not([data-theme]) {
    color-scheme: dark;
${block(themeVariables("high-contrast"), "    ")}
  }
}
:root[data-theme="light"] {
  color-scheme: light;
${block(themeVariables("light"), "  ")}
}
:root[data-theme="dark"] {
  color-scheme: dark;
${block(themeVariables("dark"), "  ")}
}
:root[data-theme="high-contrast"] {
  color-scheme: dark;
${block(themeVariables("high-contrast"), "  ")}
}
@media (prefers-reduced-motion: reduce) {
  :root {
${zeroMotion}
  }
  *, *::before, *::after {
    animation: none !important;
    transition: none !important;
    scroll-behavior: auto !important;
  }
}
@media (forced-colors: active) {
  :focus-visible { outline: 2px solid Highlight; outline-offset: 2px; }
}
`;
}
