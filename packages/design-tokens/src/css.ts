/**
 * The one typed function that emits Modbit's tokens (AFW-A11): `tokens(theme)`
 * returns the typed set, `cssVariables(theme)` flattens it to custom
 * properties, and `tokensCss()` is the stylesheet the app links: light by
 * default, dark through `prefers-color-scheme`, high contrast through
 * `prefers-contrast: more`, each overridable by `data-theme` on the root, and
 * every duration collapsed under `prefers-reduced-motion` (AFW-A14).
 */
import { PALETTES, type ColorRoles, type ThemeName } from "./palette.ts";
import { CONTROL, ELEVATION, FONT, GEOMETRY, MOTION, PRESS_SCALE, RADIUS, SPACE, TYPE, Z } from "./scale.ts";

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

/** The theme-dependent variables (colour and elevation): the only ones that differ between themes. */
export function themeVariables(theme: ThemeName): Record<string, string> {
  const t = tokens(theme);
  const out: Record<string, string> = {};
  for (const [role, value] of Object.entries(t.color)) out[`--mb-color-${kebab(role)}`] = value;
  for (const [level, value] of Object.entries(t.elevation)) out[`--mb-elevation-${level}`] = value;
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
  for (const [k, v] of Object.entries(TYPE)) {
    out[`--mb-type-${k}-size`] = `${v.size}px`;
    out[`--mb-type-${k}-line`] = `${v.line}px`;
  }
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
