/**
 * @modbit/design-tokens — the design tokens (REQ-PX-044, AFW-A11..A14 as superseded by DR-PX-2026-10-24-016):
 * colour roles for light, dark and high contrast with identical key sets (light and dark derived from ZCode's Zai tokens,
 * Apache-2.0, third-party/zcode-ui/NOTICE.md), a
 * 4 pt spacing grid, radii, control heights, a system-font type scale, motion
 * durations that collapse under prefers-reduced-motion, elevation and
 * z-layers, plus the computed WCAG AA contrast test over every pair the
 * primitives use. Canonical owner: desktop (docs/12).
 */
export { contrastRatio, luminance, mix, over, parseColor, parseHex, toHex, type Rgba } from "./contrast.ts";
export { COLOR_ROLES, PALETTES, type ColorRole, type ColorRoles, type ThemeName } from "./palette.ts";
export { CONTROL, ELEVATION, FONT, GEOMETRY, MOTION, PRESS_SCALE, RADIUS, SPACE, TYPE, UI_FONT_SIZE, UI_TEXT, Z, ZRADIUS, ZSHADOW, conversationType, type UiText } from "./scale.ts";
export { ZDL_DARK, ZDL_LIGHT, ZDL_TOKENS, type ZdlColors, type ZdlToken } from "./design-language.ts";
export { PAIRS, checkPairs, failingPairs, type Pair, type PairResult } from "./pairs.ts";
export { sharedVariables, themeVariables, tokens, tokensCss, zdlColors, type TokenSet } from "./css.ts";
export { THEME_LABEL, THEME_PREFERENCES, isThemePreference, resolveTheme, type ThemePreference } from "./theme.ts";
