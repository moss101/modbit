/** Theme preference (AFW-A13): Dark, Light, Follow system, plus a high-contrast mode; persisted by the caller. */
import type { ThemeName } from "./palette.ts";

export type ThemePreference = "system" | ThemeName;
export const THEME_PREFERENCES: readonly ThemePreference[] = ["system", "light", "dark", "high-contrast"];

export function isThemePreference(v: unknown): v is ThemePreference {
  return typeof v === "string" && (THEME_PREFERENCES as readonly string[]).includes(v);
}

export const THEME_LABEL: Record<ThemePreference, string> = { system: "Follow system", light: "Light", dark: "Dark", "high-contrast": "High contrast" };

/** The theme a preference resolves to given what the OS reports. */
export function resolveTheme(pref: ThemePreference, system: { dark: boolean; moreContrast: boolean }): ThemeName {
  if (pref !== "system") return pref;
  if (system.moreContrast) return "high-contrast";
  return system.dark ? "dark" : "light";
}
