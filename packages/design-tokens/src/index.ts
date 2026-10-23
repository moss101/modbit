/**
 * @modbit/design-tokens — Modbit's own design tokens (REQ-PX-044, AFW-A11..A14):
 * colour roles for light, dark and high contrast with identical key sets, a
 * 4 pt spacing grid, radii, control heights, a system-font type scale, motion
 * durations that collapse under prefers-reduced-motion, elevation and
 * z-layers, plus the computed WCAG AA contrast test over every pair the
 * primitives use. Canonical owner: desktop (docs/12).
 */
export { contrastRatio, luminance, mix, parseHex, toHex } from "./contrast.ts";
export { COLOR_ROLES, PALETTES, type ColorRole, type ColorRoles, type ThemeName } from "./palette.ts";
export { CONTROL, ELEVATION, FONT, GEOMETRY, MOTION, PRESS_SCALE, RADIUS, SPACE, TYPE, Z, conversationType } from "./scale.ts";
export { PAIRS, checkPairs, failingPairs, type Pair, type PairResult } from "./pairs.ts";
export { sharedVariables, themeVariables, tokens, tokensCss, type TokenSet } from "./css.ts";
export { THEME_LABEL, THEME_PREFERENCES, isThemePreference, resolveTheme, type ThemePreference } from "./theme.ts";
